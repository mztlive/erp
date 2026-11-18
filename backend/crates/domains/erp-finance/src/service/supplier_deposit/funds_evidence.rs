//! 资金证据先可靠收件，全部落地后推进同一查询窗口；不推断付款成功。

use std::collections::BTreeSet;

use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint, require_transaction};
use crate::entity::supplier_deposit::{
    DepositLedgerKind, FundsAppliedResource, FundsBalanceObservation, FundsBalanceObservationData,
    FundsEvidenceConflict, FundsEvidenceInbox, FundsEvidenceInboxState, FundsEvidenceInput,
    FundsEvidencePage, FundsEvidenceResult, FundsScanCheckpoint, FundsScanWindow, FundsTransactionDirection,
    SupplierDepositUsage,
};
use crate::repository::funds_evidence::{funds_scan_checkpoint, page_evidence};
use crate::repository::supplier_deposit_query::ledger_by_external_transaction;
use crate::repository::{FundsEvidenceExt, SupplierDepositExt};
use crate::{Error, Result};

impl SupplierDepositService {
    /// 调查供应商账户资金事实时创建固定扫描时间窗；有未完成窗口则恢复原时间窗与游标。
    /// 返回原检查点或新的首游标检查点。
    /// 未知账户、活跃窗口冲突、范围跳跃或写入失败时拒绝。
    ///
    /// 输入约定：
    /// - `account_id` / `scan_key`：已存在账户及服务器稳定窗口身份。
    /// - `window` / `executor`：固定起止范围及调用方事务。
    pub async fn begin_funds_scan(
        &self,
        account_id: &str,
        scan_key: &str,
        window: FundsScanWindow,
        executor: &mut dyn Executor,
    ) -> Result<FundsScanCheckpoint> {
        require_transaction(executor)?;
        window.validate()?;

        let account = self.account(account_id, executor).await?;
        let id = scan_id(account_id, scan_key)?;
        if let Some(existing) = self.db.funds_scan_checkpoints().find_by_id(&id, executor).await? {
            if existing.account_id != account_id
                || existing.window != window
                || existing.source_account_reference != account.identity.external_account_ref
            {
                return Err(Error::ConflictError("原资金查询窗口或账号身份不一致".into()));
            }
            return Ok(existing);
        }

        let previous = funds_scan_checkpoint(&self.db, account_id, executor).await?;
        FundsScanCheckpoint::ensure_new_window(previous.as_ref(), window)?;

        let checkpoint = FundsScanCheckpoint {
            base: BaseModel::new(id),
            account_id: account_id.into(),
            source_account_reference: account.identity.external_account_ref,
            scan_key: scan_key.into(),
            window,
            cursor: None,
            page_number: 0,
            read_complete: false,
            finished: false,
            completed_through: previous.and_then(|value| value.completed_through),
            pending_inbox_ids: vec![],
            last_page: None,
        };
        self.db.funds_scan_checkpoints().create(&checkpoint, executor).await?;
        Ok(checkpoint)
    }

    /// 接收一页资金记录时先在原事务保存全部证据，再以 CAS 推进游标；结果未知时以同一页重试恢复。
    /// 返回可靠收件后的检查点；不登记任何资金使用或核销。
    /// 窗口、游标、旧页待处理、版本、来源身份或数据库失败时拒绝。
    ///
    /// 输入约定：
    /// - `checkpoint_id` / `expected_version`：原固定窗口和所见版本。
    /// - `page` / `executor`：来源完整页和已通过账号租约屏障的同一事务。
    pub async fn save_funds_page(
        &self,
        checkpoint_id: &str,
        expected_version: u64,
        page: FundsEvidencePage,
        executor: &mut dyn Executor,
    ) -> Result<FundsScanCheckpoint> {
        require_transaction(executor)?;
        page.validate()?;

        let mut checkpoint = self.funds_checkpoint(checkpoint_id, executor).await?;
        let payload_fingerprint = fingerprint(&page)?;
        if let Some(previous) = &checkpoint.last_page
            && previous.requested_cursor == page.requested_cursor
            && previous.payload_fingerprint == payload_fingerprint
        {
            return Ok(checkpoint);
        }

        if checkpoint.base.version != expected_version {
            return Err(Error::ConflictError("资金查询检查点已被其他处理者推进".into()));
        }
        checkpoint.ensure_page(&page)?;
        self.ensure_funds_page_processed(&checkpoint, executor).await?;
        self.account(&checkpoint.account_id, executor).await?;

        let mut ids = BTreeSet::new();
        for observation in &page.observations {
            ids.insert(
                self.receive_funds_evidence(&checkpoint.account_id, observation.clone(), executor)
                    .await?
                    .base
                    .id,
            );
        }

        checkpoint.advance(&page, payload_fingerprint, ids.into_iter().collect())?;
        self.db.funds_scan_checkpoints().update(&mut checkpoint, executor).await?;
        Ok(checkpoint)
    }

    /// 已保存可靠收件且本页没有自动待办时结束原扫描窗，未核实线索继续保留。
    /// 返回完成收件范围；不证明订单未扣款、已付款或退款完成。
    /// 来源尚有后页、Received未处理、证据缺失或写入失败时拒绝。
    ///
    /// 输入约定：
    /// - `checkpoint_id` / `executor`：原检查点和已通过账号租约屏障的事务。
    pub async fn finish_funds_scan(
        &self,
        checkpoint_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<FundsScanCheckpoint> {
        require_transaction(executor)?;
        let mut checkpoint = self.funds_checkpoint(checkpoint_id, executor).await?;
        if checkpoint.finished {
            return Ok(checkpoint);
        }
        if !checkpoint.read_complete {
            return Err(Error::BusinessLogicError("资金查询尚未完整读取固定窗口".into()));
        }
        self.ensure_funds_page_processed(&checkpoint, executor).await?;

        checkpoint.finished = true;
        checkpoint.completed_through = Some(checkpoint.window.through);
        self.db.funds_scan_checkpoints().update(&mut checkpoint, executor).await?;
        Ok(checkpoint)
    }

    /// 资金证据对应的财务资源实际落库后才登记关联，不能仅凭外部消息认定付款完成。
    /// 返回已应用证据，相同资源重放不重复变更。
    /// 来源或真实资源不匹配、待核对、重复改派或写入失败时拒绝。
    ///
    /// 输入约定：
    /// - `id` / `resource`：原证据和同账户真实充值、使用或返款资源。
    /// - `executor`：记账、核销和本证据共同事务，调用方须先写账号租约屏障。
    pub async fn mark_funds_evidence_applied(
        &self,
        id: &str,
        resource: FundsAppliedResource,
        executor: &mut dyn Executor,
    ) -> Result<FundsEvidenceInbox> {
        require_transaction(executor)?;
        let mut inbox = self.funds_inbox(id, executor).await?;
        self.verify_funds_application(&inbox, &resource, executor).await?;
        let original_state = inbox.state;
        inbox.apply(resource)?;
        if inbox.state != original_state {
            self.db.funds_evidence_inbox().update(&mut inbox, executor).await?;
        }
        Ok(inbox)
    }

    /// 资金事实无法自动核实就保留证据、限制账户新增使用，不删除先前已应用的财务引用。
    /// 返回待人工核对证据；原始内容和原核销均保留。
    /// 原证据不存在、原因非法或写入失败时拒绝。
    ///
    /// 输入约定：
    /// - `id` / `reason_code`：原证据及稳定脱敏原因。
    /// - `executor`：原调查或记账同一事务。
    pub async fn mark_funds_evidence_needs_review(
        &self,
        id: &str,
        reason_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<FundsEvidenceInbox> {
        require_transaction(executor)?;
        let mut inbox = self.funds_inbox(id, executor).await?;
        inbox.needs_review(reason_code)?;
        self.pause_funds_account(&inbox.account_id, executor).await?;
        self.db.funds_evidence_inbox().update(&mut inbox, executor).await?;
        Ok(inbox)
    }

    /// 调查资金余额时追加独立的采样事实；不同采样时刻不互相校验，也不覆盖本地余额。
    /// 返回原精确采样或新增观察；同来源身份不同余额分别保留。
    /// 未知账户、归档缺失或数据库失败时拒绝。
    ///
    /// 输入约定：
    /// - `account_id` / `observation`：已核验账户和来源完整采样。
    /// - `executor`：来源证据收件事务。
    pub async fn record_funds_balance_observation(
        &self,
        account_id: &str,
        observation: FundsBalanceObservationData,
        executor: &mut dyn Executor,
    ) -> Result<FundsBalanceObservation> {
        require_transaction(executor)?;
        observation.validate()?;
        self.account(account_id, executor).await?;
        let id = balance_id(account_id, &observation)?;
        if let Some(existing) = self.db.funds_balance_observations().find_by_id(&id, executor).await? {
            return Ok(existing);
        }
        let record =
            FundsBalanceObservation { base: BaseModel::new(id), account_id: account_id.into(), observation };
        self.db.funds_balance_observations().create(&record, executor).await?;
        Ok(record)
    }

    async fn receive_funds_evidence(
        &self,
        account: &str,
        input: FundsEvidenceInput,
        executor: &mut dyn Executor,
    ) -> Result<FundsEvidenceInbox> {
        input.validate()?;
        let source_key = input.key()?;
        let id = format!("funds-evidence-{}", fingerprint(&(account, &source_key))?);
        let semantic_fingerprint = fingerprint(&input.semantic_result())?;

        if let Some(mut existing) = self.db.funds_evidence_inbox().find_by_id(&id, executor).await? {
            if existing.semantic_fingerprint != semantic_fingerprint {
                self.persist_funds_conflict(&mut existing, input, semantic_fingerprint, executor).await?;
            } else if input.source_observed_at > existing.last_observed_at {
                existing.last_observed_at = input.source_observed_at;
                self.db.funds_evidence_inbox().update(&mut existing, executor).await?;
            }
            if existing.state == FundsEvidenceInboxState::NeedsReview {
                self.pause_funds_account(account, executor).await?;
            }
            return Ok(existing);
        }

        let state = input.initial_state();
        let review_reason_code = input.initial_review_code();
        let observed_at = input.source_observed_at;
        let inbox = FundsEvidenceInbox {
            base: BaseModel::new(id),
            account_id: account.into(),
            source_key,
            semantic_fingerprint,
            input,
            state,
            first_observed_at: observed_at,
            last_observed_at: observed_at,
            applied_resource: None,
            review_reason_code,
            review_resolution: None,
        };
        if state == FundsEvidenceInboxState::NeedsReview {
            self.pause_funds_account(account, executor).await?;
        }
        self.db.funds_evidence_inbox().create(&inbox, executor).await?;
        Ok(inbox)
    }

    async fn persist_funds_conflict(
        &self,
        inbox: &mut FundsEvidenceInbox,
        input: FundsEvidenceInput,
        conflicting_fingerprint: String,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let id = format!("funds-conflict-{}", fingerprint(&(&inbox.base.id, &conflicting_fingerprint))?);
        if self.db.funds_evidence_conflicts().find_by_id(&id, executor).await?.is_none() {
            let conflict = FundsEvidenceConflict {
                base: BaseModel::new(id),
                account_id: inbox.account_id.clone(),
                inbox_id: inbox.base.id.clone(),
                original_fingerprint: inbox.semantic_fingerprint.clone(),
                conflicting_fingerprint,
                conflicting_input: input,
            };
            self.db.funds_evidence_conflicts().create(&conflict, executor).await?;
        }

        inbox.needs_review("SOURCE_FACT_CONFLICT")?;
        self.pause_funds_account(&inbox.account_id, executor).await?;
        self.db.funds_evidence_inbox().update(inbox, executor).await?;
        Ok(())
    }

    async fn ensure_funds_page_processed(
        &self,
        checkpoint: &FundsScanCheckpoint,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let rows =
            page_evidence(&self.db, &checkpoint.account_id, &checkpoint.pending_inbox_ids, executor).await?;
        if rows.len() != checkpoint.pending_inbox_ids.len() {
            return Err(Error::Internal("资金检查点缺少原已收件证据".into()));
        }
        if rows.iter().any(|row| row.state == FundsEvidenceInboxState::Received) {
            return Err(Error::BusinessLogicError("上页资金交易尚待核实应用，须先恢复原证据".into()));
        }
        Ok(())
    }

    async fn funds_checkpoint(&self, id: &str, executor: &mut dyn Executor) -> Result<FundsScanCheckpoint> {
        self.db
            .funds_scan_checkpoints()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原资金查询检查点不存在".into()))
    }
    async fn funds_inbox(&self, id: &str, executor: &mut dyn Executor) -> Result<FundsEvidenceInbox> {
        self.db
            .funds_evidence_inbox()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原资金证据不存在".into()))
    }
    async fn pause_funds_account(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        let mut account = self.account(id, executor).await?;
        account.flag_reconciliation_risk(Instant::now())?;
        self.db.supplier_deposit_accounts().update(&mut account, executor).await?;
        Ok(())
    }

    pub(super) async fn verify_funds_application(
        &self,
        inbox: &FundsEvidenceInbox,
        resource: &FundsAppliedResource,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let FundsEvidenceResult::Transaction { stable_transaction_id, amount, direction, .. } =
            &inbox.input.result
        else {
            return Err(Error::BusinessLogicError("未解释观察不能登记资金应用".into()));
        };
        match resource {
            FundsAppliedResource::RechargeBatch { batch_id } => {
                self.verify_funds_recharge(
                    batch_id,
                    &inbox.account_id,
                    stable_transaction_id,
                    *amount,
                    *direction,
                    executor,
                )
                .await?;
            },
            FundsAppliedResource::DebitUsage { usage_id } => {
                let usage = self.applied_funds_usage(usage_id, &inbox.account_id, executor).await?;
                let ledger = self
                    .db
                    .supplier_deposit_ledger()
                    .find_by_id(&usage.ledger_entry_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("真实扣款流水不存在".into()))?;
                inbox.ensure_ledger_match(&ledger, DepositLedgerKind::Debit)?;
                if *direction != FundsTransactionDirection::Debit || usage.amount != *amount {
                    return Err(Error::ConflictError("使用金额与来源扣款不一致".into()));
                }
            },
            FundsAppliedResource::ReturnUsage { usage_id } => {
                self.applied_funds_usage(usage_id, &inbox.account_id, executor).await?;
                let ledger = ledger_by_external_transaction(
                    &self.db,
                    &inbox.account_id,
                    stable_transaction_id,
                    executor,
                )
                .await?
                .ok_or_else(|| Error::NotFound("真实账户返款流水不存在".into()))?;
                inbox.ensure_ledger_match(&ledger, DepositLedgerKind::Return)?;
                if *direction != FundsTransactionDirection::Return
                    || ledger.original_usage_id.as_ref() != Some(usage_id)
                {
                    return Err(Error::ConflictError("账户返款与原使用关联不一致".into()));
                }
            },
        }
        Ok(())
    }

    async fn verify_funds_recharge(
        &self,
        batch_id: &str,
        account_id: &str,
        stable_transaction_id: &str,
        amount: Amount,
        direction: FundsTransactionDirection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let batch = self
            .db
            .supplier_deposit_batches()
            .find_by_id(batch_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("真实到账批次不存在".into()))?;
        if direction != FundsTransactionDirection::Recharge
            || batch.account_id != account_id
            || batch.amount != amount
            || batch.evidence.external_transaction_id != stable_transaction_id
        {
            return Err(Error::ConflictError("到账批次与来源交易不一致".into()));
        }
        Ok(())
    }

    async fn applied_funds_usage(
        &self,
        id: &str,
        account: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositUsage> {
        self.db
            .supplier_deposit_usages()
            .find_by_id(id, executor)
            .await?
            .filter(|usage| usage.account_id == account)
            .ok_or_else(|| Error::NotFound("真实资金使用不存在或归属不一致".into()))
    }
}

fn scan_id(account: &str, scan_key: &str) -> Result<String> {
    if scan_key.trim().is_empty() || scan_key != scan_key.trim() || scan_key.len() > 256 {
        return Err(Error::ValidationError("资金窗口身份无效".into()));
    }
    Ok(format!("funds-scan-{}", fingerprint(&(account, scan_key))?))
}

fn balance_id(account: &str, input: &FundsBalanceObservationData) -> Result<String> {
    Ok(format!(
        "funds-balance-{}",
        fingerprint(&(
            account,
            &input.observation_id,
            input.balance,
            input.currency,
            input.observed_at,
            &input.source_revision,
            &input.archive.content_fingerprint
        ))?
    ))
}
