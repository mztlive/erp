//! 正常扣款与核销原子应用；异常资金在明确回滚后可靠保留，不重放未知提交。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use erp_core::money::Amount;
use erp_finance::entity::supplier_deposit::{
    DepositLedgerKind, DepositRechargeEvidence, DepositTransactionEvidence, FundsAppliedResource,
    FundsEvidenceInbox, FundsEvidenceResult, FundsTransactionDirection, SupplierDepositLedgerEntry,
    SupplierDepositRechargeRequest, SupplierDepositUsage,
};
use erp_finance::repository::funds_evidence::FundsEvidenceExt;
use erp_finance::repository::funds_matching::{
    bank_paid_recharge_candidate, ledger_by_external_transaction, unique_original_usage,
};
use erp_finance::repository::supplier_deposit_query::reservation_for_execution;
use erp_finance::repository::{PayableExt, SupplierDepositExt};
use erp_finance::service::supplier_deposit::SupplierDepositService;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, for_external_order};
use erp_supply::ports::connector::order::OrderKey;
use erp_supply::repository::{SupplierFulfillmentExt, SupplierFulfillmentOrderRepositoryExt};
use persistence_core::{Executor, Transactional};
use sha2::{Digest, Sha256};

use super::application::{
    ApplicationMode, CashApplication, FundsApplicationSteps, apply_evidence, reviewable,
};
use super::process::FundsInvestigationRun;
use super::store::{MongoFundsInvestigationStore, SYSTEM_ACTOR};
use crate::{Error, Result};

struct Transaction<'a> {
    id: &'a str,
    order: Option<&'a str>,
    direction: FundsTransactionDirection,
    amount: Amount,
    evidence: &'a str,
    occurred_at: Instant,
    original: Option<&'a str>,
    bank_reference: Option<&'a str>,
}

impl<'a> Transaction<'a> {
    fn from_inbox(inbox: &'a FundsEvidenceInbox) -> Result<Self> {
        let FundsEvidenceResult::Transaction {
            stable_transaction_id,
            external_order_id,
            direction,
            amount,
            occurred_at,
            evidence_reference,
            original_transaction_id,
            bank_reference,
            ..
        } = &inbox.input.result
        else {
            return Err(Error::BusinessLogicError("资金证据尚未解释为真实交易".into()));
        };
        Ok(Self {
            id: stable_transaction_id,
            order: external_order_id.as_deref(),
            direction: *direction,
            amount: *amount,
            evidence: evidence_reference,
            occurred_at: *occurred_at,
            original: original_transaction_id.as_deref(),
            bank_reference: bank_reference.as_deref(),
        })
    }
    fn evidence(&self) -> Result<DepositTransactionEvidence> {
        Ok(DepositTransactionEvidence {
            external_transaction_id: self.id.into(),
            external_order_id: self
                .order
                .ok_or_else(|| Error::BusinessLogicError("真实交易缺少原订单关联".into()))?
                .into(),
            evidence_reference: self.evidence.into(),
            occurred_at: self.occurred_at,
        })
    }
}

impl MongoFundsInvestigationStore {
    /// 应用一笔可靠接收的资金证据，正常扣款、核销和任务在同一事务提交。
    /// # 参数
    /// `evidence` 为服务端已保存的原证据，实际处理状态在事务内重读。
    /// # 返回
    /// 已完成应用、异常保留或终态重放的分类计数。
    /// # 错误
    /// 存储、租约或提交未知时返回原错误；提交未知不会自动再次执行资金步骤。
    pub async fn apply_received(&self, evidence: FundsEvidenceInbox) -> Result<FundsInvestigationRun> {
        match self.application_transaction(&evidence.base.id, ApplicationMode::Complete).await {
            Ok(report) => Ok(report),
            Err(error) if reviewable(&error) => self.retain_application_failure(&evidence).await,
            Err(error) => Err(error),
        }
    }

    async fn application_transaction(
        &self,
        id: &str,
        mode: ApplicationMode,
    ) -> Result<FundsInvestigationRun> {
        let store = self.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    store.fence(executor).await?;
                    let inbox = store
                        .db
                        .funds_evidence_inbox()
                        .find_by_id(&id, executor)
                        .await?
                        .filter(|record| record.account_id == store.account_id)
                        .ok_or_else(|| Error::NotFound("原资金证据不存在".into()))?;
                    apply_evidence(&store, &inbox, mode, executor).await
                })
            })
            .await
    }

    async fn retain_application_failure(
        &self,
        evidence: &FundsEvidenceInbox,
    ) -> Result<FundsInvestigationRun> {
        if matches!(
            &evidence.input.result,
            FundsEvidenceResult::Transaction { direction: FundsTransactionDirection::Debit, .. }
        ) {
            match self.application_transaction(&evidence.base.id, ApplicationMode::PreserveDebit).await {
                Ok(report) => return Ok(report),
                Err(error) if reviewable(&error) => {},
                Err(error) => return Err(error),
            }
        }
        self.review(&evidence.base.id, "FUNDS_APPLICATION_NEEDS_REVIEW").await
    }

    async fn apply_cash(
        &self,
        inbox: &FundsEvidenceInbox,
        executor: &mut dyn Executor,
    ) -> Result<CashApplication> {
        let transaction = Transaction::from_inbox(inbox)?;
        if let Some(existing) =
            ledger_by_external_transaction(&self.db, &self.account_id, transaction.id, executor).await?
        {
            if transaction.direction == FundsTransactionDirection::Return
                && let Some(original) = transaction.original
            {
                let original_usage = match existing.original_usage_id.as_deref() {
                    Some(id) => self.db.supplier_deposit_usages().find_by_id(id, executor).await?,
                    None => None,
                };
                let expected =
                    ledger_by_external_transaction(&self.db, &self.account_id, original, executor).await?;
                if original_usage.as_ref().map(|usage| usage.ledger_entry_id.as_str())
                    != expected.as_ref().map(|entry| entry.base.id.as_str())
                    || original_usage.is_none()
                {
                    return Ok(CashApplication::Review("RETURN_ORIGINAL_TRANSACTION_CONFLICT"));
                }
            }
            return existing_resource(&existing, &transaction);
        }
        match transaction.direction {
            FundsTransactionDirection::Debit => self.apply_debit(&transaction, executor).await,
            FundsTransactionDirection::Return => self.apply_return(&transaction, executor).await,
            FundsTransactionDirection::Recharge => self.apply_recharge(&transaction, executor).await,
        }
    }

    async fn apply_debit(
        &self,
        transaction: &Transaction<'_>,
        executor: &mut dyn Executor,
    ) -> Result<CashApplication> {
        let Some(external) = transaction.order else {
            return Ok(CashApplication::Review("MISSING_ORDER_ID"));
        };
        let Some(execution) = self.original_execution(external, executor).await? else {
            return Ok(CashApplication::Review("UNMAPPED_EXTERNAL_ORDER"));
        };
        let Some(reservation) = reservation_for_execution(&self.db, &execution.base.id, executor).await?
        else {
            return Ok(CashApplication::Review("MISSING_ORIGINAL_RESERVATION"));
        };
        if reservation.account_id != self.account_id {
            return Ok(CashApplication::Review("CROSS_ACCOUNT_RESERVATION"));
        }
        let usage = SupplierDepositService::new(self.db.clone())
            .record_debit(
                &reservation.base.id,
                &operation(transaction.id),
                transaction.amount,
                transaction.evidence()?,
                SYSTEM_ACTOR,
                executor,
            )
            .await?;
        self.audit("use", executor).await?;
        Ok(CashApplication::Applied(FundsAppliedResource::DebitUsage { usage_id: usage.base.id }))
    }

    async fn apply_return(
        &self,
        transaction: &Transaction<'_>,
        executor: &mut dyn Executor,
    ) -> Result<CashApplication> {
        let Some(external) = transaction.order else {
            return Ok(CashApplication::Review("MISSING_RETURN_ORDER"));
        };
        let Some(usage) =
            unique_original_usage(&self.db, &self.account_id, external, transaction.original, executor)
                .await?
        else {
            return Ok(CashApplication::Review("AMBIGUOUS_OR_MISSING_ORIGINAL_DEBIT"));
        };
        let result = SupplierDepositService::new(self.db.clone())
            .return_usage(
                &usage.base.id,
                &operation(transaction.id),
                transaction.amount,
                transaction.evidence()?,
                SYSTEM_ACTOR,
                executor,
            )
            .await?;
        self.sync_task(&result, executor).await?;
        self.audit("refund", executor).await?;
        Ok(CashApplication::Applied(FundsAppliedResource::ReturnUsage { usage_id: result.base.id }))
    }

    async fn apply_recharge(
        &self,
        transaction: &Transaction<'_>,
        executor: &mut dyn Executor,
    ) -> Result<CashApplication> {
        let Some(bank_reference) = transaction.bank_reference else {
            return Ok(CashApplication::Review("MISSING_BANK_RECHARGE_LINK"));
        };
        let Some(request) = bank_paid_recharge_candidate(
            &self.db,
            &self.account_id,
            bank_reference,
            transaction.amount,
            executor,
        )
        .await?
        else {
            return Ok(CashApplication::Review("UNMATCHED_BANK_PAID_RECHARGE"));
        };
        let evidence = self.recharge_evidence(request, transaction, executor).await?;
        let batch = SupplierDepositService::new(self.db.clone())
            .receive_recharge(
                &self.account_id,
                &operation(transaction.id),
                transaction.amount,
                evidence,
                SYSTEM_ACTOR,
                executor,
            )
            .await?;
        self.audit("receive_recharge", executor).await?;
        Ok(CashApplication::Applied(FundsAppliedResource::RechargeBatch { batch_id: batch.base.id }))
    }

    async fn recharge_evidence(
        &self,
        request: SupplierDepositRechargeRequest,
        transaction: &Transaction<'_>,
        executor: &mut dyn Executor,
    ) -> Result<DepositRechargeEvidence> {
        let payment_id = request
            .supplier_payment_id
            .clone()
            .ok_or_else(|| Error::NotFound("原充值银行付款不存在".into()))?;
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(&payment_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原充值银行付款不存在".into()))?;
        Ok(DepositRechargeEvidence {
            approved_request_id: request.base.id,
            approval_reference: request
                .approval_reference
                .ok_or_else(|| Error::NotFound("原充值批准依据不存在".into()))?,
            supplier_payment_id: payment_id,
            bank_reference: transaction
                .bank_reference
                .ok_or_else(|| Error::BusinessLogicError("原充值缺少银行流水关联".into()))?
                .into(),
            bank_receipt_asset_id: payment
                .bank_receipt_asset_id
                .ok_or_else(|| Error::NotFound("原充值银行回单不存在".into()))?
                .to_string(),
            external_transaction_id: transaction.id.into(),
            external_evidence_reference: transaction.evidence.into(),
            received_at: transaction.occurred_at,
        })
    }

    async fn original_execution(
        &self,
        external: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApiOrderExecution>> {
        if let Some(execution) = for_external_order(&self.db, &self.connection_id, external, executor).await?
        {
            return Ok(Some(execution));
        }
        let root = self
            .db
            .supplier_fulfillment_orders()
            .find_for_connection(
                &SupplierApiConnectionId::new(&self.connection_id),
                &OrderKey::External(external.into()),
                executor,
            )
            .await?;
        match root {
            Some(root) => {
                Ok(self.db.supplier_api_order_executions().find_by_id(&root.base.id, executor).await?)
            },
            None => Ok(None),
        }
    }

    async fn settle_debit(&self, usage_id: &str, executor: &mut dyn Executor) -> Result<()> {
        let usage = self
            .db
            .supplier_deposit_usages()
            .find_by_id(usage_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原资金使用不存在".into()))?;
        if usage.settled_amount.checked_sub(usage.reversed_settlement_amount)
            != usage.amount.checked_sub(usage.returned_amount)
        {
            SupplierDepositService::new(self.db.clone())
                .settle_verified_usage(usage_id, &operation(usage_id), SYSTEM_ACTOR, executor)
                .await?;
            self.audit("settle", executor).await?;
        }
        Ok(())
    }

    async fn sync_task(&self, usage: &SupplierDepositUsage, executor: &mut dyn Executor) -> Result<()> {
        let reservation = self
            .db
            .supplier_deposit_reservations()
            .find_by_id(&usage.reservation_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原资金占用不存在".into()))?;
        match self.payment_tasks.sync(&reservation.basis.payment_request_id, executor).await {
            Ok(()) => Ok(()),
            Err(error) if reviewable(&error) => {
                let mut account = self
                    .db
                    .supplier_deposit_accounts()
                    .find_by_id(&self.account_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("原资金账户不存在".into()))?;
                account.flag_reconciliation_risk(Instant::now())?;
                self.db.supplier_deposit_accounts().update(&mut account, executor).await?;
                Ok(())
            },
            Err(error) => Err(error),
        }
    }

    /// 将无法自动解释的原资金证据交由财务核对，限制新增使用。
    /// # 参数
    /// `inbox`、`reason` 为原证据身份和稳定脱敏原因。
    /// # 返回
    /// 返回一项待核对计数；不修改已发生资金事实。
    /// # 错误
    /// 租约、证据状态、持久化或提交失败时返回错误。
    pub async fn review(&self, inbox: &str, reason: &str) -> Result<FundsInvestigationRun> {
        let store = self.clone();
        let id = inbox.to_string();
        let reason = reason.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    store.fence(executor).await?;
                    FundsApplicationSteps::review(&store, &id, &reason, executor).await
                })
            })
            .await?;
        Ok(FundsInvestigationRun { needs_review: 1, ..Default::default() })
    }
}

#[async_trait]
impl FundsApplicationSteps for MongoFundsInvestigationStore {
    async fn cash(&self, inbox: &FundsEvidenceInbox, executor: &mut dyn Executor) -> Result<CashApplication> {
        self.apply_cash(inbox, executor).await
    }

    async fn settle(&self, usage_id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.settle_debit(usage_id, executor).await
    }

    async fn payment_task(&self, usage_id: &str, executor: &mut dyn Executor) -> Result<()> {
        let usage = self
            .db
            .supplier_deposit_usages()
            .find_by_id(usage_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原资金使用不存在".into()))?;
        let reservation = self
            .db
            .supplier_deposit_reservations()
            .find_by_id(&usage.reservation_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原资金占用不存在".into()))?;
        self.payment_tasks.sync(&reservation.basis.payment_request_id, executor).await
    }

    async fn complete(
        &self,
        inbox_id: &str,
        resource: FundsAppliedResource,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        SupplierDepositService::new(self.db.clone())
            .mark_funds_evidence_applied(inbox_id, resource, executor)
            .await?;
        self.audit("funds.receive", executor).await
    }

    async fn review(&self, inbox_id: &str, reason: &str, executor: &mut dyn Executor) -> Result<()> {
        SupplierDepositService::new(self.db.clone())
            .mark_funds_evidence_needs_review(inbox_id, reason, executor)
            .await?;
        self.audit("funds.review", executor).await
    }
}

fn operation(transaction: &str) -> String {
    format!("funds-{}", hex::encode(Sha256::digest(transaction.as_bytes())))
}

fn existing_resource(
    entry: &SupplierDepositLedgerEntry,
    transaction: &Transaction<'_>,
) -> Result<CashApplication> {
    let expected = match transaction.direction {
        FundsTransactionDirection::Recharge => DepositLedgerKind::Recharge,
        FundsTransactionDirection::Debit => DepositLedgerKind::Debit,
        FundsTransactionDirection::Return => DepositLedgerKind::Return,
    };
    if entry.kind != expected
        || entry.amount != transaction.amount
        || (entry.kind != DepositLedgerKind::Recharge
            && Some(entry.evidence.external_order_id.as_str()) != transaction.order)
    {
        return Ok(CashApplication::Review("EXTERNAL_TRANSACTION_LEDGER_CONFLICT"));
    }
    Ok(CashApplication::Applied(match entry.kind {
        DepositLedgerKind::Recharge => {
            FundsAppliedResource::RechargeBatch { batch_id: entry.base.id.clone() }
        },
        DepositLedgerKind::Debit => FundsAppliedResource::DebitUsage { usage_id: entry.base.id.clone() },
        DepositLedgerKind::Return => FundsAppliedResource::ReturnUsage {
            usage_id: entry
                .original_usage_id
                .clone()
                .ok_or_else(|| Error::BusinessLogicError("原返款缺少使用关联".into()))?,
        },
        DepositLedgerKind::RechargeReversal => {
            return Ok(CashApplication::Review("UNSUPPORTED_RECHARGE_REVERSAL"));
        },
    }))
}
