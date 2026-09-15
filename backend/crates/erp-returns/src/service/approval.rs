//! 本域审批状态与最终通过守卫；工作流动作枚举由组合层分派。
use crate::entity::returns::{
    CustomerRefund, CustomerRefundStatus, PaymentReversal, PaymentReversalStatus, ReceiptReversal,
    ReceiptReversalStatus, SupplierRefund, SupplierRefundStatus,
};
use crate::{Error, Result};

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// # 参数
/// * `refund` - 待提交退款单
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_customer_refund_approval(refund: &mut CustomerRefund) -> Result<u32> {
    Ok(refund.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `refund` - 审批中的退款单
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_customer_refund_to_draft(refund: &mut CustomerRefund) -> Result<()> {
    Ok(refund.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_posting(refund: &CustomerRefund) -> Result<()> {
    if refund.status != CustomerRefundStatus::InApproval {
        return Err(Error::ConflictError("只有审批中的客户退款单可以由最终通过动作过账".to_string()));
    }
    Ok(())
}

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// # 参数
/// * `refund` - 待提交退款单
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_supplier_refund_approval(refund: &mut SupplierRefund) -> Result<u32> {
    Ok(refund.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `refund` - 审批中的退款单
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_supplier_refund_to_draft(refund: &mut SupplierRefund) -> Result<()> {
    Ok(refund.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_supplier_refund_final_approve_posting(refund: &SupplierRefund) -> Result<()> {
    if refund.status != SupplierRefundStatus::InApproval {
        return Err(Error::ConflictError("只有审批中的供应商退款单可以由最终通过动作过账".to_string()));
    }
    Ok(())
}

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// # 参数
/// * `reversal` - 待提交冲正单
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_receipt_reversal_approval(reversal: &mut ReceiptReversal) -> Result<u32> {
    Ok(reversal.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `reversal` - 审批中的冲正单
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_receipt_reversal_to_draft(reversal: &mut ReceiptReversal) -> Result<()> {
    Ok(reversal.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_receipt_reversal_final_approve_posting(reversal: &ReceiptReversal) -> Result<()> {
    if reversal.status != ReceiptReversalStatus::InApproval {
        return Err(Error::ConflictError("只有审批中的回款冲正单可以由最终通过动作过账".to_string()));
    }
    Ok(())
}

/// 提交并启动：冻结 `approval_subject_version` 并进入 `IN_APPROVAL`。
///
/// # 参数
/// * `reversal` - 待提交冲正单
///
/// # 返回
/// 返回冻结后的提交版本。
///
/// # 错误
/// 非草稿或版本溢出时返回冲突。
pub fn start_payment_reversal_approval(reversal: &mut PaymentReversal) -> Result<u32> {
    Ok(reversal.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `reversal` - 审批中的冲正单
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_payment_reversal_to_draft(reversal: &mut PaymentReversal) -> Result<()> {
    Ok(reversal.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_payment_reversal_final_approve_posting(reversal: &PaymentReversal) -> Result<()> {
    if reversal.status != PaymentReversalStatus::InApproval {
        return Err(Error::ConflictError("只有审批中的付款冲正单可以由最终通过动作过账".to_string()));
    }
    Ok(())
}
