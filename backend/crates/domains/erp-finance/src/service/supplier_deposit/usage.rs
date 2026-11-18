//! 真实资金扣款首先入账；无法核销时保留占用授权锁与对账风险。

use entity_core::BaseModel;
use erp_core::money::Amount;
use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint, identity, replay, require_transaction};
use crate::entity::supplier_deposit::{
    DepositBatchAllocation, DepositLedgerKind, DepositTransactionEvidence, SupplierDepositLedgerEntry,
    SupplierDepositReservation, SupplierDepositUsage,
};
use crate::repository::SupplierDepositExt;
use crate::{Error, Result};

impl SupplierDepositService {
    /// 保存稳定交易对应的真实扣款，不因超批丢弃资金事实。
    /// 返回原资金使用回执；不自动伪造银行付款。
    /// 缺稳定交易/订单证据、异载荷、原占用缺失或写入失败时拒绝。
    ///
    /// 输入约定：
    /// - `reservation_id` / `operation_key`：原占用及独立资金操作身份。
    /// - `amount` / `evidence`：已核实扣款与原单关联证据。
    /// - `actor` / `executor`：核验人和当前资金事务。
    pub async fn record_debit(
        &self,
        reservation_id: &str,
        operation_key: &str,
        amount: Amount,
        evidence: DepositTransactionEvidence,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositUsage> {
        require_transaction(executor)?;
        validate_evidence(&evidence, actor, amount)?;

        let mut reservation = self.reservation(reservation_id, executor).await?;
        let id = identity(&reservation.account_id, operation_key)?;
        let payload_fingerprint = fingerprint(&(reservation_id, amount, &evidence))?;
        if let Some(existing) = self.db.supplier_deposit_ledger().find_by_id(&id, executor).await? {
            replay(&payload_fingerprint, &existing.payload_fingerprint)?;
            return self.debit_receipt_usage(&id, executor).await;
        }

        let covered = amount.min(reservation.remaining()?);
        let mut account = self.account(&reservation.account_id, executor).await?;
        account.debit(amount, covered)?;
        let batches = self.consume_batches(&reservation, covered, executor).await?;
        if covered > Amount::zero() {
            reservation.consume(covered)?;
        }

        let usage = SupplierDepositUsage {
            base: BaseModel::new(id.clone()),
            account_id: reservation.account_id.clone(),
            reservation_id: reservation_id.into(),
            ledger_entry_id: id.clone(),
            amount,
            settled_amount: Amount::zero(),
            returned_amount: Amount::zero(),
            reversed_settlement_amount: Amount::zero(),
            batches: batches.clone(),
            payment_allocation_ids: vec![],
        };
        let entry = SupplierDepositLedgerEntry {
            base: BaseModel::new(id),
            account_id: account.base.id.clone(),
            operation_key: operation_key.into(),
            payload_fingerprint,
            kind: DepositLedgerKind::Debit,
            amount,
            reservation_id: Some(reservation_id.into()),
            original_usage_id: None,
            batch_allocations: batches,
            evidence,
            actor_id: actor.into(),
        };

        self.db.supplier_deposit_accounts().update(&mut account, executor).await?;
        self.db.supplier_deposit_reservations().update(&mut reservation, executor).await?;
        self.db.supplier_deposit_ledger().create(&entry, executor).await?;
        self.db.supplier_deposit_usages().create(&usage, executor).await?;
        Ok(usage)
    }

    async fn debit_receipt_usage(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositUsage> {
        self.db
            .supplier_deposit_usages()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::Internal("扣款回执缺少原使用记录".into()))
    }

    pub(super) async fn reservation(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositReservation> {
        self.db
            .supplier_deposit_reservations()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原预存款占用不存在".into()))
    }

    async fn consume_batches(
        &self,
        reservation: &SupplierDepositReservation,
        amount: Amount,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DepositBatchAllocation>> {
        let mut remaining = amount;
        let mut offset = reservation.consumed_amount.checked_add(reservation.released_amount);
        let mut allocations = Vec::new();
        for original in &reservation.batches {
            if remaining == Amount::zero() {
                break;
            }
            if offset >= original.amount {
                offset = offset.checked_sub(original.amount);
                continue;
            }
            let from_batch = remaining.min(original.amount.checked_sub(offset));
            offset = Amount::zero();

            let mut batch = self
                .db
                .supplier_deposit_batches()
                .find_by_id(&original.batch_id, executor)
                .await?
                .ok_or_else(|| Error::Internal("原占用充值批次缺失".into()))?;
            if batch.account_id != reservation.account_id {
                return Err(Error::Internal("原占用批次跨资金账号".into()));
            }
            batch.consume(from_batch)?;
            self.db.supplier_deposit_batches().update(&mut batch, executor).await?;
            remaining = remaining.checked_sub(from_batch);
            allocations
                .push(DepositBatchAllocation { batch_id: original.batch_id.clone(), amount: from_batch });
        }

        if remaining != Amount::zero() {
            return Err(Error::Internal("资金占用批次不守恒".into()));
        }
        Ok(allocations)
    }
}

fn validate_evidence(evidence: &DepositTransactionEvidence, actor: &str, amount: Amount) -> Result<()> {
    if amount <= Amount::zero()
        || [&evidence.external_transaction_id, &evidence.external_order_id, &evidence.evidence_reference]
            .iter()
            .any(|value| value.trim().is_empty() || value.as_str() != value.trim() || value.len() > 256)
        || actor.trim().is_empty()
        || actor != actor.trim()
        || actor.len() > 128
    {
        return Err(Error::ValidationError("真实资金交易必须有正金额、稳定交易、原单、证据和核验人".into()));
    }
    Ok(())
}
