//! 结案前按原稳定交易重新核实人工补齐的真实资源，不重新执行任何记账。

use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint};
use crate::entity::supplier_deposit::{
    DepositLedgerKind, DepositReconciliationResolution, FundsAppliedResource, FundsEvidenceInbox,
    FundsEvidenceResult, FundsEvidenceReviewResolution, SupplierDepositReconciliation,
};
use crate::repository::funds_evidence::FundsEvidenceExt;
use crate::repository::funds_matching::ledger_by_external_transaction;
use crate::repository::supplier_deposit_resolution::{
    recharge_batch_for_transaction, reviewed_funds_evidence, usage_for_ledger,
};
use crate::{Error, Result};

impl SupplierDepositService {
    pub(super) async fn recover_reviewed_funds(
        &self,
        account: &str,
        resolution: &DepositReconciliationResolution,
        basis: &SupplierDepositReconciliation,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for mut inbox in reviewed_funds_evidence(&self.db, account, executor).await? {
            let resource = self.reviewed_resource(&inbox, basis, executor).await?;
            self.verify_funds_application(&inbox, &resource, executor).await?;

            let proof = FundsEvidenceReviewResolution {
                operation_key: resolution.operation_key.clone(),
                payload_fingerprint: fingerprint(&(
                    &inbox.base.id,
                    &resource,
                    &resolution.payload_fingerprint,
                ))?,
                evidence_asset_id: resolution.evidence_asset_id.clone(),
                actor_id: resolution.actor_id.clone(),
                reviewed_at: resolution.resolved_at,
                original_reason_code: None,
            };
            inbox.resolve_reviewed(resource, proof)?;
            self.db.funds_evidence_inbox().update(&mut inbox, executor).await?;
        }
        Ok(())
    }

    async fn reviewed_resource(
        &self,
        inbox: &FundsEvidenceInbox,
        basis: &SupplierDepositReconciliation,
        executor: &mut dyn Executor,
    ) -> Result<FundsAppliedResource> {
        let FundsEvidenceResult::Transaction { stable_transaction_id, .. } = &inbox.input.result else {
            return Err(Error::BusinessLogicError("未解释资金仍待取得稳定交易和实际记账依据".into()));
        };
        let ledger =
            ledger_by_external_transaction(&self.db, &inbox.account_id, stable_transaction_id, executor)
                .await?
                .ok_or_else(|| {
                    Error::BusinessLogicError("原待核验资金尚未匹配真实到账、扣款或返款流水".into())
                })?;
        let reservation = match ledger.reservation_id.as_deref() {
            Some(id) => Some(self.reservation(id, executor).await?),
            None => None,
        };
        inbox.ensure_review_coverage(basis, &ledger, reservation.as_ref())?;

        match ledger.kind {
            DepositLedgerKind::Recharge => {
                let batch = recharge_batch_for_transaction(
                    &self.db,
                    &inbox.account_id,
                    stable_transaction_id,
                    executor,
                )
                .await?
                .ok_or_else(|| Error::NotFound("原到账交易缺少同账户真实充值批次".into()))?;
                Ok(FundsAppliedResource::RechargeBatch { batch_id: batch.base.id })
            },
            DepositLedgerKind::Debit => {
                let usage = usage_for_ledger(&self.db, &inbox.account_id, &ledger.base.id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("原扣款缺少同账户真实使用记录".into()))?;
                Ok(FundsAppliedResource::DebitUsage { usage_id: usage.base.id })
            },
            DepositLedgerKind::Return => Ok(FundsAppliedResource::ReturnUsage {
                usage_id: ledger
                    .original_usage_id
                    .ok_or_else(|| Error::NotFound("原返款缺少真实使用关联".into()))?,
            }),
            DepositLedgerKind::RechargeReversal => {
                Err(Error::BusinessLogicError("充值冲正不能替代原到账交易结案".into()))
            },
        }
    }
}
