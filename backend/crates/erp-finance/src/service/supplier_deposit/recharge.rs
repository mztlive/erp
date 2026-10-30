//! 已批准真实银行充值和供应商到账核对；不产生采购核销。

use entity_core::BaseModel;
use erp_core::money::Amount;
use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint, identity, replay, require_transaction};
use crate::entity::payable::{SupplierPaymentPurpose, SupplierPaymentStatus};
use crate::entity::supplier_deposit::{
    DepositBatchAllocation, DepositLedgerKind, DepositRechargeEvidence, DepositRechargeStatus,
    DepositTransactionEvidence, SupplierDepositAccount, SupplierDepositBatch, SupplierDepositLedgerEntry,
};
use crate::repository::{PayableExt, SupplierDepositExt};
use crate::{Error, Result};

impl SupplierDepositService {
    /// 在实际银行付款与供应商到账均已核实后登记可用充值一次。
    /// # 参数
    /// * `account_id` / `operation_key` - 账户及稳定操作身份。
    /// * `amount` / `evidence` - 实际到账金额与完整证据。
    /// * `actor` / `executor` - 核验人和调用方事务。
    /// # 返回
    /// 返回唯一到账批次；重放返回原批次。
    /// # 错误
    /// 银行付款不真实、不属于充值、归属/金额/审批依据不符或重复异载荷时拒绝。
    pub async fn receive_recharge(
        &self,
        account_id: &str,
        operation_key: &str,
        amount: Amount,
        evidence: DepositRechargeEvidence,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositBatch> {
        require_transaction(executor)?;
        let evidence = evidence.normalize()?;
        let id = identity(account_id, operation_key)?;
        let payload_fingerprint = fingerprint(&(account_id, amount, &evidence))?;
        if let Some(entry) = self.db.supplier_deposit_ledger().find_by_id(&id, executor).await? {
            replay(&payload_fingerprint, &entry.payload_fingerprint)?;
            return self
                .db
                .supplier_deposit_batches()
                .find_by_id(&id, executor)
                .await?
                .ok_or_else(|| Error::Internal("原充值回执缺少到账批次".into()));
        }
        let mut account = self.account(account_id, executor).await?;
        let mut request = self.recharge_request(account_id, &evidence.approved_request_id, executor).await?;
        if request.status != DepositRechargeStatus::BankPaid
            || request.supplier_payment_id.as_ref() != Some(&evidence.supplier_payment_id)
            || request.approval_reference.as_ref() != Some(&evidence.approval_reference)
            || request.amount != amount
        {
            return Err(Error::BusinessLogicError("充值申请尚未完成实际银行付款或到账依据不符".into()));
        }
        self.verify_recharge_payment(&account, amount, &evidence, executor).await?;
        let batch = SupplierDepositBatch::new(id.clone(), account_id.into(), amount, evidence.clone())?;
        let entry =
            recharge_ledger(&id, account_id, operation_key, payload_fingerprint, amount, &evidence, actor)?;
        account.credit(amount)?;
        request.status = DepositRechargeStatus::Received;
        request.received_batch_id = Some(batch.base.id.clone());
        self.db.supplier_deposit_accounts().update(&mut account, executor).await?;
        self.db.supplier_deposit_batches().create(&batch, executor).await?;
        self.db.supplier_deposit_ledger().create(&entry, executor).await?;
        self.db.supplier_deposit_recharge_requests().update(&mut request, executor).await?;
        Ok(batch)
    }
    async fn verify_recharge_payment(
        &self,
        account: &SupplierDepositAccount,
        amount: Amount,
        evidence: &DepositRechargeEvidence,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(&evidence.supplier_payment_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("充值银行付款不存在".into()))?;
        let expected = SupplierPaymentPurpose::SupplierDepositRecharge {
            recharge_request_id: evidence.approved_request_id.clone(),
            deposit_account_id: account.base.id.clone(),
            approval_reference: evidence.approval_reference.clone(),
        };
        if payment.purpose != expected
            || payment.status != SupplierPaymentStatus::Posted
            || payment.effective_amount() != amount
            || payment.supplier_id.as_ref() != account.identity.supplier_id
            || payment.payer_party_id.as_deref() != Some(&account.identity.payer_party_id)
            || payment.bank_reference.as_deref() != Some(&evidence.bank_reference)
            || payment.bank_receipt_asset_id.as_ref().map(AsRef::<str>::as_ref)
                != Some(&evidence.bank_receipt_asset_id)
        {
            return Err(Error::BusinessLogicError(
                "充值用途、批准依据、银行付款、主体或到账金额不一致".into(),
            ));
        }
        Ok(())
    }
}

fn recharge_ledger(
    id: &str,
    account: &str,
    operation: &str,
    payload_fingerprint: String,
    amount: Amount,
    evidence: &DepositRechargeEvidence,
    actor: &str,
) -> Result<SupplierDepositLedgerEntry> {
    if actor.trim().is_empty() {
        return Err(Error::ValidationError("到账核验人不能为空".into()));
    }
    Ok(SupplierDepositLedgerEntry {
        base: BaseModel::new(id.into()),
        account_id: account.into(),
        operation_key: operation.into(),
        payload_fingerprint,
        kind: DepositLedgerKind::Recharge,
        amount,
        reservation_id: None,
        original_usage_id: None,
        batch_allocations: vec![DepositBatchAllocation { batch_id: id.into(), amount }],
        evidence: DepositTransactionEvidence {
            external_transaction_id: evidence.external_transaction_id.clone(),
            external_order_id: String::new(),
            evidence_reference: evidence.external_evidence_reference.clone(),
            occurred_at: evidence.received_at,
        },
        actor_id: actor.into(),
    })
}
