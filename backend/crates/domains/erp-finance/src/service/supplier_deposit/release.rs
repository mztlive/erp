//! 发送风险持久化及无风险额度释放。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint, identity, replay, require_transaction};
use crate::entity::supplier_deposit::{DepositExposure, DepositReleaseReceipt, SupplierDepositReservation};
use crate::repository::{PurchasePaymentRequestExt, SupplierDepositExt};
use crate::{Error, Result};

impl SupplierDepositService {
    /// 在登记首个外部写意图的同一事务标记资金可能已发送。
    /// # 参数
    /// * `id` - 本执行组原占用。
    /// * `executor` - 包含外部动作意图登记的同一事务。
    /// # 返回
    /// 后续任何超时都不能释放原占用。
    /// # 错误
    /// 原占用已释放、缺失或持久化失败时拒绝。
    pub async fn mark_external_write_sent(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        require_transaction(executor)?;
        let mut reservation = self.reservation(id, executor).await?;
        self.fence_execution_qualification(&reservation, executor).await?;
        if reservation.exposure != DepositExposure::PossiblySent {
            reservation.mark_sent()?;
        }
        self.db.supplier_deposit_reservations().update(&mut reservation, executor).await?;
        Ok(())
    }

    async fn fence_execution_qualification(
        &self,
        reservation: &SupplierDepositReservation,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut account = self.account(&reservation.account_id, executor).await?;
        if !account.enabled
            || account.reconciliation_required
            || account.reserved_amount < reservation.remaining()?
        {
            return Err(Error::BusinessLogicError("预存款账户资格变化或原占用不足".into()));
        }
        let mut request = self
            .request_for_reservation(&account.base.id, &account.identity, &reservation.basis, executor)
            .await?;
        let mut authorization = self
            .db
            .supplier_deposit_authorizations()
            .find_by_id(&reservation.basis.authorization_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原资金授权不存在".into()))?;
        authorization.ensure_active(
            &account.base.id,
            &reservation.basis.purchase_order_id,
            reservation.basis.authorization_version,
            Instant::now(),
        )?;
        self.db.supplier_deposit_accounts().update(&mut account, executor).await?;
        self.db.purchase_payment_requests().update(&mut request, executor).await?;
        self.db.supplier_deposit_authorizations().update(&mut authorization, executor).await?;
        Ok(())
    }

    /// 释放有明确原执行风险消解证据的剩余占用和申请额度。
    /// # 参数
    /// * `id` / `amount` - 原占用与释放金额。
    /// * `clearance` - 组合层已核验的无扣款、冻结及后续风险依据。
    /// * `executor` - 账户、批次和申请共同事务。
    /// # 返回
    /// 返回保存历史的占用；金额不会增加账面余额。
    /// # 错误
    /// 缺原申请、未核实外部风险或超额时拒绝。
    pub async fn release_reservation(
        &self,
        id: &str,
        operation_key: &str,
        amount: Amount,
        clearance: Option<String>,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositReservation> {
        require_transaction(executor)?;
        let mut reservation = self.reservation(id, executor).await?;
        let key = identity(id, operation_key)?;
        let payload = fingerprint(&(amount, &clearance))?;
        if let Some(previous) = reservation.release_receipts.get(&key) {
            replay(&payload, &previous.payload_fingerprint)?;
            return Ok(reservation);
        }
        if actor.trim().is_empty() || reservation.release_receipts.len() >= 1000 {
            return Err(Error::ValidationError("释放操作人缺失或历史释放次数超限".into()));
        }
        let receipt = DepositReleaseReceipt {
            amount,
            payload_fingerprint: payload,
            clearance_evidence_reference: clearance.clone(),
            actor_id: actor.into(),
            verification: None,
        };
        if let Some(evidence) = clearance {
            reservation.clear_exposure(evidence)?;
        }
        let previous_offset = reservation.consumed_amount.checked_add(reservation.released_amount);
        reservation.release(amount)?;
        let mut account = self.account(&reservation.account_id, executor).await?;
        let mut request = self
            .db
            .purchase_payment_requests()
            .find_by_id(&reservation.basis.payment_request_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原付款申请不存在".into()))?;
        request.release_deposit(id, amount)?;
        account.release(amount)?;
        self.release_batches(&reservation, previous_offset, amount, executor).await?;
        self.db.supplier_deposit_accounts().update(&mut account, executor).await?;
        self.db.purchase_payment_requests().update(&mut request, executor).await?;
        reservation.release_receipts.insert(key, receipt);
        self.db.supplier_deposit_reservations().update(&mut reservation, executor).await?;
        Ok(reservation)
    }

    async fn release_batches(
        &self,
        reservation: &SupplierDepositReservation,
        mut offset: Amount,
        mut remaining: Amount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for allocation in &reservation.batches {
            if remaining == Amount::zero() {
                break;
            }
            if offset >= allocation.amount {
                offset = offset.checked_sub(allocation.amount);
                continue;
            }
            let released = remaining.min(allocation.amount.checked_sub(offset));
            offset = Amount::zero();
            let mut batch = self
                .db
                .supplier_deposit_batches()
                .find_by_id(&allocation.batch_id, executor)
                .await?
                .filter(|batch| batch.account_id == reservation.account_id)
                .ok_or_else(|| Error::Internal("原占用批次缺失或跨账户".into()))?;
            batch.release(released)?;
            self.db.supplier_deposit_batches().update(&mut batch, executor).await?;
            remaining = remaining.checked_sub(released);
        }
        if remaining != Amount::zero() {
            return Err(Error::Internal("原占用批次金额不守恒".into()));
        }
        Ok(())
    }
}
