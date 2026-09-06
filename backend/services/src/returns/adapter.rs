//! `CustomerRefund` / `SupplierRefund` / `ReceiptReversal` / `PaymentReversal` 审批业务 Adapter。
//!
//! 必须显式声明合同 §4.4 / 阶段 04 §6 的全部适配器字段。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式。
//! 资金类 `PENDING_REVIEW` 已收敛为 `IN_APPROVAL`，不得再走通用状态更新。

mod customer_refund;
mod payment_reversal;
mod receipt_reversal;
mod supplier_refund;

use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;

use super::dto::DocumentApprovalDefinitionView;

pub use self::customer_refund::{
    build_customer_refund_snapshot, customer_refund_adapter, customer_refund_object_readable,
    customer_refund_responsible_org_id, customer_refund_start_command, customer_refund_subject_ref,
    document_approval_view, ensure_final_approve_posting, execute_customer_refund_domain_action,
    require_frozen_binding, start_approval_command_kind, start_customer_refund_approval,
    CustomerRefundAdapter,
};
pub use self::payment_reversal::{
    build_payment_reversal_snapshot, ensure_payment_reversal_final_approve_posting,
    execute_payment_reversal_domain_action, payment_reversal_adapter, payment_reversal_approval_view,
    payment_reversal_object_readable, payment_reversal_responsible_org_id, payment_reversal_start_command,
    payment_reversal_start_command_kind, payment_reversal_subject_ref, require_payment_reversal_binding,
    start_payment_reversal_approval, PaymentReversalAdapter,
};
pub use self::receipt_reversal::{
    build_receipt_reversal_snapshot, ensure_receipt_reversal_final_approve_posting,
    execute_receipt_reversal_domain_action, receipt_reversal_adapter, receipt_reversal_approval_view,
    receipt_reversal_object_readable, receipt_reversal_responsible_org_id, receipt_reversal_start_command,
    receipt_reversal_start_command_kind, receipt_reversal_subject_ref, require_receipt_reversal_binding,
    start_receipt_reversal_approval, ReceiptReversalAdapter,
};
pub use self::supplier_refund::{
    build_supplier_refund_snapshot, ensure_supplier_refund_final_approve_posting,
    execute_supplier_refund_domain_action, require_supplier_refund_binding, start_supplier_refund_approval,
    supplier_refund_adapter, supplier_refund_approval_view, supplier_refund_object_readable,
    supplier_refund_responsible_org_id, supplier_refund_start_command, supplier_refund_start_command_kind,
    supplier_refund_subject_ref, SupplierRefundAdapter,
};

/// 详情最近审批历史条数上限。完整历史走分页端点。
pub const RECENT_HISTORY_LIMIT: usize = 8;

/// 由冻结绑定投影定义摘要。节点详情不在单据详情展开。
fn definition_view_from_binding(binding: &ApprovalDefinitionBinding) -> DocumentApprovalDefinitionView {
    DocumentApprovalDefinitionView {
        id: binding.approval_process_definition_id.as_ref().to_string(),
        name: String::new(),
        version: binding.approval_definition_version,
        nodes: Vec::new(),
    }
}
