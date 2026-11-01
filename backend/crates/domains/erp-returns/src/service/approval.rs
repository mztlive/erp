//! 本域审批状态与最终通过守卫；工作流动作枚举由组合层分派。
use crate::entity::returns::{
    CustomerRefund, CustomerRefundStatus, PaymentReversal, PaymentReversalStatus, ReceiptReversal,
    ReceiptReversalStatus, SupplierRefund, SupplierRefundStatus,
};
use crate::{Error, Result};

/// 四单据共用的最终通过守卫：非审批中即按单据名拒绝。
///
/// # 参数
/// * `in_approval` - 待过账单据是否处于审批中
/// * `document_label` - 冲突文案中的单据名（各单据专属文案不变）
///
/// # 返回
/// 审批中时返回成功。
///
/// # 错误
/// 非审批中时返回 `ConflictError`。
fn ensure_in_approval(in_approval: bool, document_label: &str) -> Result<()> {
    if !in_approval {
        return Err(Error::ConflictError(format!("只有审批中的{document_label}可以由最终通过动作过账")));
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
/// 非草稿或审批版本溢出时返回 `Logic`。
pub fn start_customer_refund_approval(refund: &mut CustomerRefund) -> Result<u32> {
    Ok(refund.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `refund` - 审批中的退款单。
///
/// # 返回
/// 成功时单据回到草稿，审批版本不回退。
///
/// # 错误
/// 非审批中时返回 `Logic`。
pub fn cancel_customer_refund_to_draft(refund: &mut CustomerRefund) -> Result<()> {
    Ok(refund.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 参数
/// * `refund` - 待过账单据。
///
/// # 返回
/// 单据处于审批中时返回成功。
///
/// # 错误
/// 非审批中时返回 `ConflictError`。
pub fn ensure_final_approve_posting(refund: &CustomerRefund) -> Result<()> {
    ensure_in_approval(refund.status == CustomerRefundStatus::InApproval, "客户退款单")
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
/// 非草稿或审批版本溢出时返回 `Logic`。
pub fn start_supplier_refund_approval(refund: &mut SupplierRefund) -> Result<u32> {
    Ok(refund.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `refund` - 审批中的退款单。
///
/// # 返回
/// 成功时单据回到草稿，审批版本不回退。
///
/// # 错误
/// 非审批中时返回 `Logic`。
pub fn cancel_supplier_refund_to_draft(refund: &mut SupplierRefund) -> Result<()> {
    Ok(refund.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 参数
/// * `refund` - 待过账单据。
///
/// # 返回
/// 单据处于审批中时返回成功。
///
/// # 错误
/// 非审批中时返回 `ConflictError`。
pub fn ensure_supplier_refund_final_approve_posting(refund: &SupplierRefund) -> Result<()> {
    ensure_in_approval(refund.status == SupplierRefundStatus::InApproval, "供应商退款单")
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
/// 非草稿或审批版本溢出时返回 `Logic`。
pub fn start_receipt_reversal_approval(reversal: &mut ReceiptReversal) -> Result<u32> {
    Ok(reversal.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `reversal` - 审批中的冲正单。
///
/// # 返回
/// 成功时单据回到草稿，审批版本不回退。
///
/// # 错误
/// 非审批中时返回 `Logic`。
pub fn cancel_receipt_reversal_to_draft(reversal: &mut ReceiptReversal) -> Result<()> {
    Ok(reversal.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 参数
/// * `reversal` - 待过账单据。
///
/// # 返回
/// 单据处于审批中时返回成功。
///
/// # 错误
/// 非审批中时返回 `ConflictError`。
pub fn ensure_receipt_reversal_final_approve_posting(reversal: &ReceiptReversal) -> Result<()> {
    ensure_in_approval(reversal.status == ReceiptReversalStatus::InApproval, "回款冲正单")
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
/// 非草稿或审批版本溢出时返回 `Logic`。
pub fn start_payment_reversal_approval(reversal: &mut PaymentReversal) -> Result<u32> {
    Ok(reversal.start_approval()?)
}

/// 撤回审批：回到草稿，且 `subject_version` 不回退。
///
/// # 参数
/// * `reversal` - 审批中的冲正单。
///
/// # 返回
/// 成功时单据回到草稿，审批版本不回退。
///
/// # 错误
/// 非审批中时返回 `Logic`。
pub fn cancel_payment_reversal_to_draft(reversal: &mut PaymentReversal) -> Result<()> {
    Ok(reversal.cancel_approval()?)
}

/// 最终通过过账前置：仅 `IN_APPROVAL` 可进入过账。
///
/// # 参数
/// * `reversal` - 待过账单据。
///
/// # 返回
/// 单据处于审批中时返回成功。
///
/// # 错误
/// 非审批中时返回 `ConflictError`。
pub fn ensure_payment_reversal_final_approve_posting(reversal: &PaymentReversal) -> Result<()> {
    ensure_in_approval(reversal.status == PaymentReversalStatus::InApproval, "付款冲正单")
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{CustomerRefundId, PaymentReversalId, ReceiptReversalId, SupplierRefundId};

    use super::{
        ensure_final_approve_posting, ensure_payment_reversal_final_approve_posting,
        ensure_receipt_reversal_final_approve_posting, ensure_supplier_refund_final_approve_posting,
    };
    use crate::Error;
    use crate::entity::returns::{
        CustomerRefundStatus, PaymentReversalStatus, ReceiptReversalStatus, SupplierRefundStatus,
        customer_refund, payment_reversal, receipt_reversal, supplier_refund,
    };

    /// 四单据最终守卫共用同一实现：草稿拒绝文案各异，审批中放行。
    #[test]
    fn final_posting_guards_share_semantics_with_per_document_messages() {
        let mut customer = customer_refund::CustomerRefund::new(
            CustomerRefundId::new("cr-1"),
            customer_refund::tests::data(),
            "actor-1",
        )
        .unwrap();
        let err = ensure_final_approve_posting(&customer).unwrap_err();
        assert_eq!(err.to_string(), "数据冲突: 只有审批中的客户退款单可以由最终通过动作过账");

        let mut supplier = supplier_refund::SupplierRefund::new(
            SupplierRefundId::new("sr-1"),
            supplier_refund::tests::data(),
            "actor-1",
        )
        .unwrap();
        let err = ensure_supplier_refund_final_approve_posting(&supplier).unwrap_err();
        assert_eq!(err.to_string(), "数据冲突: 只有审批中的供应商退款单可以由最终通过动作过账");

        let mut receipt = receipt_reversal::ReceiptReversal::new(
            ReceiptReversalId::new("rr-1"),
            receipt_reversal::tests::data(),
            "actor-1",
        )
        .unwrap();
        let err = ensure_receipt_reversal_final_approve_posting(&receipt).unwrap_err();
        assert_eq!(err.to_string(), "数据冲突: 只有审批中的回款冲正单可以由最终通过动作过账");

        let mut payment = payment_reversal::PaymentReversal::new(
            PaymentReversalId::new("pr-1"),
            payment_reversal::tests::data(),
            "actor-1",
        )
        .unwrap();
        let err = ensure_payment_reversal_final_approve_posting(&payment).unwrap_err();
        assert_eq!(err.to_string(), "数据冲突: 只有审批中的付款冲正单可以由最终通过动作过账");

        for started in [
            customer.start_approval().is_ok(),
            supplier.start_approval().is_ok(),
            receipt.start_approval().is_ok(),
            payment.start_approval().is_ok(),
        ] {
            assert!(started);
        }
        assert!(ensure_final_approve_posting(&customer).is_ok());
        assert!(ensure_supplier_refund_final_approve_posting(&supplier).is_ok());
        assert!(ensure_receipt_reversal_final_approve_posting(&receipt).is_ok());
        assert!(ensure_payment_reversal_final_approve_posting(&payment).is_ok());

        for (customer_status, supplier_status, receipt_status, payment_status) in [
            (
                CustomerRefundStatus::Posted,
                SupplierRefundStatus::Posted,
                ReceiptReversalStatus::Posted,
                PaymentReversalStatus::Posted,
            ),
            (
                CustomerRefundStatus::Reversed,
                SupplierRefundStatus::Reversed,
                ReceiptReversalStatus::Reversed,
                PaymentReversalStatus::Reversed,
            ),
        ] {
            customer.status = customer_status;
            supplier.status = supplier_status;
            receipt.status = receipt_status;
            payment.status = payment_status;
            for (result, label) in [
                (ensure_final_approve_posting(&customer), "客户退款单"),
                (ensure_supplier_refund_final_approve_posting(&supplier), "供应商退款单"),
                (ensure_receipt_reversal_final_approve_posting(&receipt), "回款冲正单"),
                (ensure_payment_reversal_final_approve_posting(&payment), "付款冲正单"),
            ] {
                assert!(matches!(result, Err(Error::ConflictError(message))
                    if message == format!("只有审批中的{label}可以由最终通过动作过账")));
            }
        }
    }
}
