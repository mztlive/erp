//! 执行组占用与付款申请额度锁定的共同事务。

use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint, identity, replay, require_transaction};
use crate::entity::supplier_deposit::{
    DepositBatchAllocation, DepositExposure, DepositReservationBasis, SupplierDepositReservation,
};
use crate::repository::supplier_deposit_query::allocatable_batches;
use crate::repository::{PurchasePaymentRequestExt, SupplierDepositExt};
use crate::{Error, Result};

impl SupplierDepositService {
    /// 原子锁定全额执行组资金与逐单批准额度，重放保留原资金批次。
    /// # 参数
    /// * `account_id` / `operation_key` - 资金账户和稳定操作身份。
    /// * `basis` - 冻结审批、采购修订、预算和财务授权。
    /// * `actor` / `now` - 执行身份和时间。
    /// * `executor` - 跨账户、批次、申请和授权共同事务。
    /// # 返回
    /// 返回唯一可恢复的资金占用。
    /// # 错误
    /// 异载荷、归属、版本、授权、金额、余额或事务失败时拒绝。
    pub async fn reserve(
        &self,
        account_id: &str,
        operation_key: &str,
        basis: DepositReservationBasis,
        actor: &str,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositReservation> {
        require_transaction(executor)?;
        let id = identity(account_id, operation_key)?;
        let payload_fingerprint = fingerprint(&(account_id, &basis))?;
        if let Some(existing) = self.db.supplier_deposit_reservations().find_by_id(&id, executor).await? {
            replay(&payload_fingerprint, &existing.payload_fingerprint)?;
            return Ok(existing);
        }

        let amount = basis.total()?;
        let mut account = self.account(account_id, executor).await?;
        let mut request =
            self.request_for_reservation(account_id, &account.identity, &basis, executor).await?;

        let mut authorization = self
            .db
            .supplier_deposit_authorizations()
            .find_by_id(&basis.authorization_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("财务余额执行授权不存在".into()))?;
        authorization.commit(
            account_id,
            &basis.purchase_order_id,
            amount,
            basis.authorization_version,
            now,
        )?;

        request.reserve_deposit(&id, &basis.payable_account_id, basis.payment_request_version, amount)?;
        account.reserve(amount)?;
        let batches = self.reserve_batches(account_id, amount, executor).await?;

        let reservation = SupplierDepositReservation {
            base: BaseModel::new(id),
            account_id: account_id.into(),
            operation_key: operation_key.into(),
            payload_fingerprint,
            basis,
            amount,
            consumed_amount: Amount::zero(),
            released_amount: Amount::zero(),
            batches,
            exposure: DepositExposure::NotSent,
            clearance_evidence_reference: None,
            actor_id: checked_actor(actor)?,
            release_receipts: Default::default(),
        };

        self.db.supplier_deposit_accounts().update(&mut account, executor).await?;
        self.db.supplier_deposit_authorizations().update(&mut authorization, executor).await?;
        self.db.purchase_payment_requests().update(&mut request, executor).await?;
        self.db.supplier_deposit_reservations().create(&reservation, executor).await?;
        Ok(reservation)
    }

    async fn reserve_batches(
        &self,
        account: &str,
        amount: Amount,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DepositBatchAllocation>> {
        let mut batches = allocatable_batches(&self.db, account, executor).await?;
        let allocations = DepositBatchAllocation::reserve_from(&mut batches, amount)?;
        for batch in &mut batches {
            if allocations.iter().any(|allocation| allocation.batch_id == batch.base.id) {
                self.db.supplier_deposit_batches().update(batch, executor).await?;
            }
        }
        Ok(allocations)
    }
}

fn checked_actor(actor: &str) -> Result<String> {
    if actor.trim().is_empty() || actor.len() > 128 {
        return Err(Error::ValidationError("资金执行身份无效".into()));
    }
    Ok(actor.into())
}
