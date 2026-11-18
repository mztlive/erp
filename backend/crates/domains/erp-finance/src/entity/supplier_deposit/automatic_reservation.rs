//! 自动余额占用只使用原逐单批准和明确的自动授权，不冒充人工任务。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::{DepositAuthorizationMode, DepositReservationBasis, SupplierDepositAuthorization, add, positive};
use crate::entity::payable::{
    PurchasePaymentRequest, PurchasePaymentRequestLine, PurchasePaymentRequestStatus,
};

/// 服务端冻结执行组的真实资金归属与全部批准预算。
pub struct DepositAutomaticReservationData {
    pub account_id: String,
    pub purchase_order_id: String,
    pub purchase_revision_id: String,
    pub execution_group_id: String,
    pub approved_goods_amount: Amount,
    pub approved_freight: Amount,
}

impl DepositAutomaticReservationData {
    /// 选择能覆盖本执行组全额预算的唯一原批准申请。
    /// 返回唯一具备未被其他执行占用额度的原申请。
    /// 失败条件：原归属不合法、无全额批准或存在多个合格申请时拒绝，须指定出纳处理。
    ///
    /// 输入约定：
    /// - `candidates`：同主体、账号、采购和当前修订的已批准候选。
    pub fn request<'a>(
        &self,
        candidates: &'a [PurchasePaymentRequest],
    ) -> Result<&'a PurchasePaymentRequest> {
        let total = self.total()?;
        let mut selected = None;
        for request in candidates {
            let line = self.request_line(request)?;
            if request.available_to_execute(&line.payable_account_id)? < total {
                continue;
            }
            if selected.replace(request).is_some() {
                return Err(Error::from("本执行组存在多个合格余额付款申请，请指定出纳明确选择"));
            }
        }
        selected
            .ok_or_else(|| Error::from("本执行组缺少全额已批准余额付款申请，请先完成付款审批或指定出纳处理"))
    }

    /// 选择当前有限额且适用本采购的自动授权，人工授权不进入候选。
    /// 返回首个能够覆盖完整预算的有效自动条款，尚未消费额度。
    /// 失败条件：不存在有效自动条款时明确要求指定出纳执行，不创建假授权。
    ///
    /// 输入约定：
    /// - `grants`：按稳定顺序读取的账号自动授权。
    /// - `now`：本次服务器时间。
    pub fn authorization<'a>(
        &self,
        grants: &'a [SupplierDepositAuthorization],
        now: Instant,
    ) -> Result<&'a SupplierDepositAuthorization> {
        let total = self.total()?;
        for grant in grants {
            if grant.mode != DepositAuthorizationMode::Automatic {
                continue;
            }
            let mut candidate = grant.clone();
            if candidate
                .commit(&self.account_id, &self.purchase_order_id, total, grant.grant_version, now)
                .is_ok()
            {
                return Ok(grant);
            }
        }
        Err(Error::from("自动余额授权缺失或额度不足，请指定出纳执行余额方式"))
    }

    /// 根据服务器已核验的逐单申请和自动条款构造专属资金占用依据。
    /// 返回可交给拥有领域占用事务的完整依据。
    /// 账号、采购或人工授权混入时拒绝。
    ///
    /// 输入约定：
    /// - `request` / `grant`：已选择的原批准申请及明确自动条款。
    pub fn basis(
        &self,
        request: &PurchasePaymentRequest,
        grant: &SupplierDepositAuthorization,
    ) -> Result<DepositReservationBasis> {
        let line = self.request_line(request)?;
        if grant.account_id != self.account_id || grant.mode != DepositAuthorizationMode::Automatic {
            return Err(Error::from("自动余额占用不能借用其他账号或人工执行授权"));
        }
        Ok(DepositReservationBasis {
            payment_request_id: request.base.id.clone(),
            payment_request_version: request.submission_version,
            payable_account_id: line.payable_account_id.clone(),
            purchase_order_id: self.purchase_order_id.clone(),
            purchase_revision_id: self.purchase_revision_id.clone(),
            execution_group_id: self.execution_group_id.clone(),
            approved_goods_amount: self.approved_goods_amount,
            approved_freight: self.approved_freight,
            authorization_id: grant.base.id.clone(),
            authorization_version: grant.grant_version,
        })
    }

    fn request_line<'a>(
        &self,
        request: &'a PurchasePaymentRequest,
    ) -> Result<&'a PurchasePaymentRequestLine> {
        request.require_deposit_account(&self.account_id)?;
        if !matches!(
            request.status,
            PurchasePaymentRequestStatus::Approved | PurchasePaymentRequestStatus::PartiallyPaid
        ) {
            return Err(Error::from("原余额付款申请尚未批准或已结束"));
        }
        request
            .lines
            .iter()
            .find(|line| {
                line.purchase_order_id == self.purchase_order_id
                    && line.purchase_revision_id == self.purchase_revision_id
            })
            .ok_or_else(|| Error::from("原余额付款申请不包含本采购生效修订"))
    }

    fn total(&self) -> Result<Amount> {
        if self.approved_goods_amount < Amount::zero() || self.approved_freight < Amount::zero() {
            return Err(Error::from("执行组批准商品及运费不得为负数"));
        }
        let total = add(self.approved_goods_amount, self.approved_freight)?;
        positive(total)?;
        Ok(total)
    }
}
