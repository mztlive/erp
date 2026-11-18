//! 预存款和普通付款共用的逐申请执行额度锁。

use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::types::{positive, required, sum};
use super::{
    PaymentRequestAllocation, PaymentRequestDepositReservation, PaymentRequestExecutionMethod,
    PurchasePaymentRequest, PurchasePaymentRequestStatus,
};
use crate::entity::supplier_deposit::{DepositAccountIdentity, DepositReservationBasis};
use crate::{Error as FinanceError, Result as FinanceResult};

impl PurchasePaymentRequest {
    /// 核对原预存款占用对应的批准主体、提交版本及采购修订。
    /// # 参数
    /// * `account_id` / `identity` - 当前资金账户及真实归属。
    /// * `basis` - 原执行组冻结的付款与采购依据。
    /// # 返回
    /// 冻结依据与当前申请一致时成功；已付申请仍可核验原执行。
    /// # 错误
    /// 账户依据缺失、申请关闭或冻结归属变化时保留原分类与首错。
    pub(crate) fn ensure_deposit_reservation(
        &self,
        account_id: &str,
        identity: &DepositAccountIdentity,
        basis: &DepositReservationBasis,
    ) -> FinanceResult<()> {
        let frozen_account = self.require_deposit_account(account_id)?;
        if &frozen_account.identity != identity {
            return Err(FinanceError::BusinessLogicError("预存款真实账号身份与原批准快照不一致".into()));
        }

        if !matches!(
            self.status,
            PurchasePaymentRequestStatus::Approved
                | PurchasePaymentRequestStatus::PartiallyPaid
                | PurchasePaymentRequestStatus::Paid
        ) {
            return Err(FinanceError::BusinessLogicError("原付款申请未批准或授权已关闭".into()));
        }
        if self.supplier_id != identity.supplier_id
            || self.payer_party_id != identity.payer_party_id
            || self.submission_version != basis.payment_request_version
        {
            return Err(FinanceError::BusinessLogicError(
                "预存款与原付款申请主体、供应商或审批版本不一致".into(),
            ));
        }

        let line = self
            .lines
            .iter()
            .find(|line| line.payable_account_id == basis.payable_account_id)
            .ok_or_else(|| FinanceError::BusinessLogicError("执行采购不在原付款申请内".into()))?;
        if line.purchase_order_id != basis.purchase_order_id
            || line.purchase_revision_id != basis.purchase_revision_id
        {
            return Err(FinanceError::BusinessLogicError("资金执行采购修订与原批准来源不一致".into()));
        }
        Ok(())
    }

    /// 返回未被预存款占用且未支付的批准额度。
    /// # 参数
    /// * `account_id` - 原申请采购应付身份。
    /// # 返回
    /// 返回银行、留抵及新占用共同适用的可执行额度。
    /// # 错误
    /// 不在申请内、投影异常或金额溢出时拒绝。
    pub fn available_to_execute(&self, account_id: &str) -> Result<Amount> {
        let line = self
            .lines
            .iter()
            .find(|line| line.payable_account_id == account_id)
            .ok_or_else(|| Error::from("采购应付不在付款申请范围内"))?;
        let locked = sum(self
            .deposit_reservations
            .values()
            .filter(|item| item.payable_account_id == account_id)
            .map(|item| item.amount))?;
        if locked > line.remaining() {
            return Err(Error::from("预存款占用超过付款批准剩余额度"));
        }
        Ok(line.remaining().checked_sub(locked))
    }

    /// 在原逐单审批范围内锁定执行组额度。
    /// # 参数
    /// * `reservation_id` - 稳定资金占用身份。
    /// * `account_id` - 原申请采购应付。
    /// * `version` - 冻结审批提交版本。
    /// * `amount` - 全额预算。
    /// # 返回
    /// 成功增加专属占用，净已付不变。
    /// # 错误
    /// 未批准、版本漂移、重复占用或不足时拒绝。
    pub fn reserve_deposit(
        &mut self,
        reservation_id: &str,
        account_id: &str,
        version: u32,
        amount: Amount,
    ) -> Result<()> {
        if !matches!(self.data.execution_method, PaymentRequestExecutionMethod::SupplierDeposit { .. }) {
            return Err(Error::from("原付款申请未批准预存款执行方式"));
        }
        positive(amount)?;
        required(reservation_id.into(), "预存款占用", 256)?;
        if !matches!(
            self.status,
            PurchasePaymentRequestStatus::Approved | PurchasePaymentRequestStatus::PartiallyPaid
        ) || self.submission_version != version
            || self.deposit_reservations.contains_key(reservation_id)
        {
            return Err(Error::from("付款申请未批准、审批版本已变化或已存在原占用"));
        }
        if amount > self.available_to_execute(account_id)? {
            return Err(Error::from("付款申请可执行额度不足"));
        }

        self.deposit_reservations.insert(
            reservation_id.into(),
            PaymentRequestDepositReservation { payable_account_id: account_id.into(), amount },
        );
        Ok(())
    }

    /// 释放明确无外部风险的原执行额度。
    /// # 参数
    /// * `reservation_id` - 原资金占用身份。
    /// * `amount` - 经资金服务证明可释放的金额。
    /// # 返回
    /// 该金额可再次经正式执行使用。
    /// # 错误
    /// 身份缺失、非正或超出原占用时拒绝。
    pub fn release_deposit(&mut self, reservation_id: &str, amount: Amount) -> Result<()> {
        positive(amount)?;
        let item = self
            .deposit_reservations
            .get_mut(reservation_id)
            .ok_or_else(|| Error::from("付款申请缺少原预存款占用"))?;
        if amount > item.amount {
            return Err(Error::from("金额超过原申请预存款占用"));
        }
        let remaining = item.amount.checked_sub(amount);
        if remaining == Amount::zero() {
            self.deposit_reservations.remove(reservation_id);
        } else {
            item.amount = remaining;
        }
        Ok(())
    }

    /// 将真实扣款对应的额度从占用切换为正式已付。
    /// # 参数
    /// * `reservation_id` - 原执行组占用。
    /// * `amount` - 与本次正式核销完全相同的金额。
    /// # 返回
    /// 原占用同额减少、净已付增加，调用方须原子写入核销。
    /// # 错误
    /// 原占用不足或申请失效时拒绝，内存实体保持原值。
    pub fn settle_deposit(&mut self, reservation_id: &str, amount: Amount) -> Result<()> {
        if !matches!(self.data.execution_method, PaymentRequestExecutionMethod::SupplierDeposit { .. }) {
            return Err(Error::from("原付款申请未批准预存款执行方式"));
        }
        let item = self
            .deposit_reservations
            .get(reservation_id)
            .ok_or_else(|| Error::from("付款申请缺少原预存款占用"))?
            .clone();

        let mut candidate = self.clone();
        candidate.release_deposit(reservation_id, amount)?;
        candidate.record_authorized_payment(&[PaymentRequestAllocation {
            payable_account_id: item.payable_account_id,
            amount,
        }])?;

        *self = candidate;
        Ok(())
    }
}
