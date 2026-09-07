//! 采购审批生命周期的单域状态转换与原错误分类。
use crate::entity::purchase_order::PurchaseOrder;
use crate::{Error, Result};
/// 提交并启动：进入 `IN_APPROVAL`，递增 `approval_subject_version`。
///
/// # 参数
/// * `order` - 待提交采购单
/// * `submission_id` - 本次冻结提交
/// * `updated_by` - 提交人
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_purchase_order_approval(
    order: &mut PurchaseOrder,
    submission_id: impl Into<String>,
    updated_by: &str,
) -> Result<u32> {
    Ok(order.start_approval(submission_id, updated_by)?)
}
/// 撤回审批：回到 `DRAFT`，且提交版本不回退。
///
/// # 参数
/// * `order` - 审批中的采购单
/// * `updated_by` - 操作人
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_purchase_order_to_draft(order: &mut PurchaseOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval(updated_by)?)
}
/// 将采购单最终生效领域守卫适配为既有 Service 冲突语义。
///
/// # 参数
/// * `order` - 待执行最终通过动作的采购单聚合
///
/// # 返回
/// 聚合主状态允许最终生效时返回 `Ok(())`。
///
/// # 错误
/// 聚合不允许最终生效时返回文本不变的 `ConflictError`。
///
/// # 关键约束
/// 仅调用实体的状态专用守卫，不附加冻结提交指针等更严格校验。
pub fn ensure_final_approve_formalize(order: &PurchaseOrder) -> Result<()> {
    order
        .ensure_can_formalize()
        .map_err(|_| Error::ConflictError("只有审批中的采购单可以由最终通过动作生效".to_string()))
}
