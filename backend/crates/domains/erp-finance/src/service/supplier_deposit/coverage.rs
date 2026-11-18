//! 新写动作入口重验原执行组资金和当前授权，不用远端余额替代。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use persistence_core::Executor;

use super::SupplierDepositService;
use crate::entity::payable::PurchasePaymentRequest;
use crate::entity::supplier_deposit::{
    DepositAccountIdentity, DepositReservationBasis, SupplierDepositAccount, SupplierDepositReservation,
};
use crate::repository::supplier_deposit_query::{reservation_for_execution, usages_for_reservation};
use crate::repository::{PurchasePaymentRequestExt, SupplierDepositExt};
use crate::{Error, Result};

impl SupplierDepositService {
    /// 核对创建或确认的全额资金保障；调查和取消不调用本门槛。
    /// # 参数
    /// * `execution_group_id` / `purchase_order_id` / `purchase_revision_id` - 冻结业务来源。
    /// * `approved_total` - 本执行组完整批准预算。
    /// * `now` / `executor` - 当前时间及调用方执行器。
    /// # 返回
    /// 返回拥有全额有效覆盖的唯一原占用。
    /// # 错误
    /// 占用缺失、归属/版本/金额变化、授权撤销或资金差异时拒绝。
    pub async fn execution_coverage(
        &self,
        execution_group_id: &str,
        purchase_order_id: &str,
        purchase_revision_id: &str,
        approved_total: Amount,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositReservation> {
        let reservation = reservation_for_execution(&self.db, execution_group_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("余额待占用：本执行组尚无专属资金保障".into()))?;
        if reservation.basis.purchase_order_id != purchase_order_id
            || reservation.basis.purchase_revision_id != purchase_revision_id
            || reservation.amount != approved_total
            || reservation.released_amount > Amount::zero()
        {
            return Err(Error::BusinessLogicError("采购修订或批准预算已变化，请核实原资金执行".into()));
        }

        let account = self.account(&reservation.account_id, executor).await?;
        if !account.enabled || account.reconciliation_required {
            return Err(Error::BusinessLogicError("预存款账户停用或资金待对账".into()));
        }
        self.request_for_reservation(&account.base.id, &account.identity, &reservation.basis, executor)
            .await?;
        let authorization = self
            .db
            .supplier_deposit_authorizations()
            .find_by_id(&reservation.basis.authorization_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("财务余额执行授权不存在".into()))?;
        authorization.ensure_active(
            &reservation.account_id,
            purchase_order_id,
            reservation.basis.authorization_version,
            now,
        )?;

        let usages = usages_for_reservation(&self.db, &reservation.base.id, executor).await?;
        let net_debits = usages.iter().try_fold(Amount::zero(), |total, usage| {
            let value = total
                .to_decimal()
                .checked_add(usage.amount.to_decimal())
                .and_then(|value| value.checked_sub(usage.returned_amount.to_decimal()))
                .ok_or_else(|| Error::BusinessLogicError("执行组资金合计溢出".into()))?;
            Amount::try_from(value).map_err(Error::from)
        })?;
        if reservation.remaining()?.checked_add(net_debits) < approved_total {
            return Err(Error::BusinessLogicError("本执行组资金占用和已核实扣款不足".into()));
        }
        Ok(reservation)
    }

    pub(super) async fn account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositAccount> {
        self.db
            .supplier_deposit_accounts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("预存款账户不存在".into()))
    }

    pub(super) async fn request_for_reservation(
        &self,
        account_id: &str,
        identity: &DepositAccountIdentity,
        basis: &DepositReservationBasis,
        executor: &mut dyn Executor,
    ) -> Result<PurchasePaymentRequest> {
        let request = self
            .db
            .purchase_payment_requests()
            .find_by_id(&basis.payment_request_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购付款申请不存在".into()))?;
        request.ensure_deposit_reservation(account_id, identity, basis)?;
        Ok(request)
    }
}
