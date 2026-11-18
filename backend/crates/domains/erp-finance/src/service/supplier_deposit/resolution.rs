//! 人工核验文件由组合层验证；资金结案仍须通过全账户事实与调查写屏障。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint, identity, require_transaction};
use crate::entity::supplier_deposit::{
    DepositReconciliationResolution, DepositReleaseReceipt, DepositReleaseVerification,
    SupplierDepositReconciliation, SupplierDepositReservation, SupplierDepositUsage,
};
use crate::repository::SupplierDepositExt;
use crate::repository::supplier_deposit_query::{ledger_balance_at, usages_for_reservation};
use crate::repository::supplier_deposit_resolution::{
    ensure_account_funds_resolved, ensure_account_risk_covered, ensure_funds_window_covered,
    latest_reservation_funds_time, outstanding_reservations, unresolved_reconciliations,
};
use crate::{Error, Result};

/// 组合层已经核实原执行终态、归属和附件的释放命令。
pub struct VerifiedDepositRelease<'a> {
    pub account: &'a str,
    pub reservation_id: &'a str,
    pub expected_version: u64,
    pub operation_key: &'a str,
    pub verification: DepositReleaseVerification,
    pub last_external_fact_at: Instant,
    pub actor_id: &'a str,
}

impl SupplierDepositService {
    /// 读取原风险释放命令的永久回执，已失效附件不阻断合法原结果查询。
    /// # 参数
    /// 原账户、占用、稳定操作、同内容核验依据、认证人及已授权查询执行器。
    /// # 返回
    /// 原命令已成功时返回原占用，否则返回空。
    /// # 错误
    /// 跨账户或同键不同内容时拒绝。
    pub async fn verified_release_result(
        &self,
        account: &str,
        id: &str,
        operation: &str,
        verification: &DepositReleaseVerification,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierDepositReservation>> {
        let reservation = self.reservation(id, executor).await?;
        if reservation.account_id != account {
            return Err(Error::NotFound("原资金占用不属于当前账户".into()));
        }
        if let Some(receipt) = reservation.release_receipts.get(&identity(id, operation)?) {
            ensure_release_replay(receipt, verification, actor)?;
            return Ok(Some(reservation));
        }
        Ok(None)
    }

    /// 以完整平衡核对及核验附件结案原差异，最后一项结案才解除账户限制。
    /// # 参数
    /// `account`、`id`、`version` 为原账户、差异及所见版本；`resolution` 为服务器核验的命令。
    /// `executor` 为权限、附件、资金及审计共用的事务。
    /// # 返回
    /// 返回保留原差异和不可变结案回执的记录。
    /// # 错误
    /// 跨账户、陈旧版本、未决资金、来源冲突、资金范围不完整或账面不守恒时拒绝。
    pub async fn resolve_reconciliation(
        &self,
        account: &str,
        id: &str,
        version: u64,
        resolution: DepositReconciliationResolution,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositReconciliation> {
        require_transaction(executor)?;
        let mut record = self.reconciliation(account, id, executor).await?;
        let mut candidate = record.clone();
        if !candidate.resolve(resolution.clone())? {
            return Ok(record);
        }

        ensure_version(record.base.version, version)?;
        let owner = self.resolution_fence(account, &resolution.operation_key, executor).await?;
        let basis = self.reconciliation(account, &resolution.balanced_reconciliation_id, executor).await?;
        record.ensure_resolution_basis(&basis, Instant::now())?;
        self.account(account, executor).await?.ensure_reconciliation_risk_observed(basis.observed_at)?;
        ensure_account_risk_covered(&self.db, account, &basis, executor).await?;
        self.recover_reviewed_funds(account, &resolution, &basis, executor).await?;
        self.ensure_resolution_funds(account, &basis, executor).await?;

        record.resolve(resolution)?;
        self.db.supplier_deposit_reconciliations().update(&mut record, executor).await?;

        let unresolved = unresolved_reconciliations(&self.db, account, executor).await?;
        let mut funds = self.account(account, executor).await?;
        funds.reconciliation_required = !unresolved.is_empty();
        self.db.supplier_deposit_accounts().update(&mut funds, executor).await?;

        self.release_resolution_fence(account, &owner, executor).await?;
        Ok(record)
    }

    /// 依据已经核实的执行终态和文件释放服务器计算的全部未使用尾款。
    /// # 参数
    /// 原账户、占用及版本、稳定操作、服务器核验依据、最后外部事实时刻及执行人。
    /// `executor` 包含原执行版本写屏障、资金释放和审计。
    /// # 返回
    /// 返回不可变释放回执；同操作同载荷重放不再次释放。
    /// # 错误
    /// 原执行未结清、核对早于外部事实、未决资金、版本变化或余额不守恒时拒绝。
    pub async fn release_verified_reservation(
        &self,
        command: VerifiedDepositRelease<'_>,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositReservation> {
        let VerifiedDepositRelease {
            account,
            reservation_id: id,
            expected_version: version,
            operation_key: operation,
            verification,
            last_external_fact_at,
            actor_id: actor,
        } = command;
        require_transaction(executor)?;
        let reservation = self.reservation(id, executor).await?;
        if reservation.account_id != account {
            return Err(Error::NotFound("原资金占用不属于当前账户".into()));
        }
        let operation_key = identity(id, operation)?;
        if let Some(receipt) = reservation.release_receipts.get(&operation_key) {
            ensure_release_replay(receipt, &verification, actor)?;
            return Ok(reservation);
        }

        ensure_version(reservation.base.version, version)?;
        let owner = self.resolution_fence(account, operation, executor).await?;
        let basis = self.reconciliation(account, &verification.reconciliation_id, executor).await?;
        basis.ensure_resolution_basis(&basis, Instant::now())?;
        self.account(account, executor).await?.ensure_reconciliation_risk_observed(basis.observed_at)?;
        ensure_account_risk_covered(&self.db, account, &basis, executor).await?;
        self.ensure_resolution_funds(account, &basis, executor).await?;

        let usages = usages_for_reservation(&self.db, id, executor).await?;
        let amount = reservation.verified_release_amount(&usages, verification.reason)?;
        let started = i64::try_from(reservation.base.created_at)
            .map_err(|_| Error::ValidationError("原资金占用开始时间越界".into()))?;
        if basis.covered_from > Instant::try_from_unix_secs(started)? {
            return Err(Error::BusinessLogicError("资金核对未覆盖原占用开始后的完整风险范围".into()));
        }
        self.ensure_release_cutoff(&usages, &basis, last_external_fact_at, executor).await?;

        let reference = format!("risk-verification:{}", fingerprint(&(id, operation, &verification))?);
        let mut result =
            self.release_reservation(id, operation, amount, Some(reference), actor, executor).await?;
        result
            .release_receipts
            .get_mut(&operation_key)
            .ok_or_else(|| Error::Internal("风险释放缺少原命令回执".into()))?
            .verification = Some(verification);
        self.db.supplier_deposit_reservations().update(&mut result, executor).await?;

        self.release_resolution_fence(account, &owner, executor).await?;
        Ok(result)
    }

    async fn ensure_resolution_funds(
        &self,
        account: &str,
        basis: &SupplierDepositReconciliation,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_account_funds_resolved(&self.db, account, executor).await?;
        ensure_funds_window_covered(&self.db, account, basis.covered_from, basis.covered_until, executor)
            .await?;

        let cutoff = ledger_balance_at(&self.db, account, basis.covered_until, executor).await?;
        let current = ledger_balance_at(&self.db, account, Instant::now(), executor).await?;
        let funds = self.account(account, executor).await?;
        let reservations = outstanding_reservations(&self.db, account, executor).await?;
        let reserved = reservations.iter().try_fold(Amount::zero(), |total, value| {
            let sum = total
                .to_decimal()
                .checked_add(value.remaining()?.to_decimal())
                .ok_or_else(|| Error::BusinessLogicError("账户占用重建金额溢出".into()))?;
            Amount::try_from(sum).map_err(Error::from)
        })?;

        if cutoff != basis.local_balance_at_cutoff
            || current != funds.book_balance
            || reserved != funds.reserved_amount
            || funds.book_balance < Amount::zero()
            || funds.available()? < Amount::zero()
        {
            return Err(Error::BusinessLogicError("对账依据已变化或账户流水、账面及可用额度不守恒".into()));
        }
        Ok(())
    }

    async fn reconciliation(
        &self,
        account: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositReconciliation> {
        self.db
            .supplier_deposit_reconciliations()
            .find_by_id(id, executor)
            .await?
            .filter(|record| record.account_id == account)
            .ok_or_else(|| Error::NotFound("核对记录不存在或不属于当前资金账户".into()))
    }

    async fn resolution_fence(
        &self,
        account: &str,
        operation: &str,
        executor: &mut dyn Executor,
    ) -> Result<(String, u64)> {
        let owner = format!("manual-resolution:{}", fingerprint(&(account, operation))?);
        let lease = self
            .claim_funds_lease(account, &owner, Instant::now(), 60, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("账户正在补查资金，请稍后按原操作重试核验".into()))?;
        Ok((owner, lease.fencing_token))
    }

    async fn release_resolution_fence(
        &self,
        account: &str,
        owner: &(String, u64),
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.release_funds_lease(account, &owner.0, owner.1, Instant::now(), executor).await
    }

    async fn ensure_release_cutoff(
        &self,
        usages: &[SupplierDepositUsage],
        basis: &SupplierDepositReconciliation,
        last_external_fact_at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if basis.covered_until < last_external_fact_at {
            return Err(Error::BusinessLogicError("资金核对早于原订单最终事实，请补查至最新终态之后".into()));
        }
        if let Some(latest) = latest_reservation_funds_time(
            &self.db,
            &basis.account_id,
            usages.first().map(|usage| usage.reservation_id.as_str()),
            executor,
        )
        .await?
            && latest > basis.covered_until
        {
            return Err(Error::BusinessLogicError("资金核对未覆盖最后一笔原执行扣款或返款".into()));
        }
        for usage in usages {
            let entry = self
                .db
                .supplier_deposit_ledger()
                .find_by_id(&usage.ledger_entry_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("原资金使用缺少真实流水".into()))?;
            if entry.account_id != basis.account_id || entry.evidence.occurred_at > basis.covered_until {
                return Err(Error::BusinessLogicError("资金核对未覆盖原执行全部扣款或返款".into()));
            }
        }
        Ok(())
    }
}

fn ensure_version(actual: u64, expected: u64) -> Result<()> {
    if actual != expected {
        return Err(Error::ConflictError("原资金记录已变化，请刷新后核对".into()));
    }
    Ok(())
}

fn ensure_release_replay(
    receipt: &DepositReleaseReceipt,
    verification: &DepositReleaseVerification,
    actor: &str,
) -> Result<()> {
    let mut expected = verification.clone();
    expected.verified_at =
        receipt.verification.as_ref().map_or(verification.verified_at, |value| value.verified_at);
    if receipt.verification.as_ref() != Some(&expected) || receipt.actor_id != actor {
        return Err(Error::ConflictError("同一风险释放操作不能更换原核验依据".into()));
    }
    Ok(())
}
