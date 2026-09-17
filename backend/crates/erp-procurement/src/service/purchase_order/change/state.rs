//! 采购变更单拥有的提交、撤回与最终生效状态守卫。
//!
//! 本层保留原因：三个入口的调用方在组合层 `erp-processes`（跨组 crate，
//! 本域不得改动），直接删除会跨组破坏调用；实体方法返回 `erp_core` 错误，
//! 本层将其映射为本域 [`Error`]，并锁定对外冲突文案，故保留为薄守卫层。
use crate::entity::purchase_order::{PurchaseChangeOrder, PurchaseChangeOrderStatus};
use crate::{Error, Result};

/// 提交并启动：进入 `IN_APPROVAL`，递增 `approval_subject_version`。
///
/// # 参数
/// * `order` - 待提交变更单
/// * `submission_id` - 本次冻结提交
/// * `target_content_hash` - 目标内容指纹
/// * `updated_by` - 提交人
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿、指纹非法或版本溢出时返回冲突。
pub fn start_purchase_change_approval(
    order: &mut PurchaseChangeOrder,
    submission_id: erp_core::ids::PurchaseChangeSubmissionId,
    target_content_hash: impl Into<String>,
    updated_by: &str,
) -> Result<u32> {
    Ok(order.start_approval(submission_id, target_content_hash, updated_by)?)
}

/// 撤回审批：回到 `DRAFT`，且提交版本不回退。
///
/// # 参数
/// * `order` - 审批中的变更单
/// * `updated_by` - 操作人
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_purchase_change_to_draft(order: &mut PurchaseChangeOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval(updated_by)?)
}

/// 最终通过前置：仅 `IN_APPROVAL` 可进入生效。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_effective(order: &PurchaseChangeOrder) -> Result<()> {
    if order.stable.status != PurchaseChangeOrderStatus::InApproval {
        return Err(Error::ConflictError("只有审批中的采购变更单可以由最终通过动作生效".to_string()));
    }
    Ok(())
}
