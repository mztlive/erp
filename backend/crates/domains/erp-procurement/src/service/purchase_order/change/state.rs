//! 采购变更单拥有的提交、撤回与最终生效状态守卫。
//!
//! 公开入口供采购编排复用，将实体错误映射为既有 Service 错误分类。
use crate::entity::purchase_order::PurchaseChangeOrder;
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
/// 非草稿、指纹非法或版本溢出时，实体错误经 `?` 变为 `Logic`。
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
/// # 返回
/// 撤回到草稿后无返回值。`approval_subject_version` 不回退。
///
/// # 错误
/// 非审批中时，实体错误经 `?` 变为 `Logic`。
pub fn cancel_purchase_change_to_draft(order: &mut PurchaseChangeOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval(updated_by)?)
}

/// 最终通过前置：仅 `IN_APPROVAL` 可进入生效。
///
/// # 参数
/// * `order` - 待最终通过的变更单
///
/// # 返回
/// 状态为审批中时返回 `Ok(())`。
///
/// # 错误
/// 状态不是审批中时返回 `ConflictError`。
pub fn ensure_final_approve_effective(order: &PurchaseChangeOrder) -> Result<()> {
    order.ensure_can_take_effect().map_err(|error| Error::ConflictError(error.to_string()))
}

/// 提交时一并更新变更原因；空白表示沿用原因。
///
/// # 参数
/// * `order` - 草稿变更单
/// * `reason` - 提交携带的变更原因
/// * `updated_by` - 提交人
///
/// # 返回
/// 原因未提供、与原值相同或更新成功时返回 `Ok(())`。
///
/// # 错误
/// 变更单不是草稿或原因超长时返回错误。
pub fn apply_change_reason(
    order: &mut PurchaseChangeOrder,
    reason: Option<&str>,
    updated_by: &str,
) -> Result<()> {
    Ok(order.update_submission_reason(reason, updated_by)?)
}

/// 作废草稿变更单。
///
/// # 参数
/// * `order` - 变更单
/// * `updated_by` - 作废人
///
/// # 返回
/// 草稿作废成功返回 `Ok(())`。
///
/// # 错误
/// 非草稿时返回冲突。
pub fn void_purchase_change(order: &mut PurchaseChangeOrder, updated_by: &str) -> Result<()> {
    order.void(updated_by).map_err(|error| Error::ConflictError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{PurchaseChangeOrderId, PurchaseOrderId, PurchaseOrderRevisionId};

    use super::*;
    use crate::entity::purchase_order::{PurchaseChangeOrderData, PurchaseChangeOrderStatus};

    fn order() -> PurchaseChangeOrder {
        PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new("change-1"),
            PurchaseChangeOrderData {
                purchase_order_id: PurchaseOrderId::new("purchase-1"),
                base_revision_id: PurchaseOrderRevisionId::new("revision-1"),
                reason: "变更原因".into(),
            },
            "buyer",
        )
        .unwrap()
    }

    #[test]
    fn change_state_adapters_preserve_conflict_and_logic_errors() {
        let mut change = order();
        assert!(matches!(
            ensure_final_approve_effective(&change),
            Err(Error::ConflictError(message)) if message == "只有审批中的采购变更单可以由最终通过动作生效"
        ));

        change.stable.status = PurchaseChangeOrderStatus::InApproval;
        ensure_final_approve_effective(&change).unwrap();

        apply_change_reason(&mut change, Some(" 变更原因 "), "other").unwrap();
        assert_eq!(change.stable.updated_by, "buyer");
        assert!(matches!(
            apply_change_reason(&mut change, Some("新原因"), "other"),
            Err(Error::Logic(error)) if error.to_string() == "只有草稿状态的采购变更单可以编辑内容"
        ));
    }
}
