//! 不可变提交材料；重新提交不能覆盖旧审批的完整审核依据。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{PurchasePaymentRequest, PurchasePaymentRequestStatus};

/// 一轮申请提交及其精确审批实例的完整脱敏材料。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct PurchasePaymentRequestSubmissionSnapshot {
    #[serde(flatten)]
    pub base: BaseModel,
    pub request_id: String,
    pub submission_version: u32,
    pub approval_instance_id: String,
    pub request: PurchasePaymentRequest,
}

impl PurchasePaymentRequestSubmissionSnapshot {
    /// 复制本轮已提交且已绑定审批实例的完整审核材料。
    ///
    /// # 参数
    /// * `request` - 当前事务已提交并写入审批绑定的申请。
    /// # 返回
    /// 返回独立的不可变副本，身份由申请与提交版本确定。
    /// # 错误
    /// 非审批中、缺少来源、提交版本或审批实例时返回错误。
    pub fn freeze(request: &PurchasePaymentRequest) -> Result<Self> {
        let instance = request
            .approval_instance_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| Error::from("付款申请提交材料缺少精确审批实例"))?;
        if request.status != PurchasePaymentRequestStatus::InApproval
            || request.submission_version == 0
            || request.submission_version != request.approval_subject_version
            || request.lines.is_empty()
            || request.payee_account.is_none()
        {
            return Err(Error::from("只有来源和审批完整的付款申请提交可以冻结材料"));
        }
        Ok(Self {
            base: BaseModel::new(format!(
                "payment-request-submission-{}-{}",
                request.base.id, request.submission_version
            )),
            request_id: request.base.id.clone(),
            submission_version: request.submission_version,
            approval_instance_id: instance.to_owned(),
            request: request.clone(),
        })
    }
}
