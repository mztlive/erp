//! `CustomerRefund` / `SupplierRefund` / `ReceiptReversal` / `PaymentReversal` 审批业务 Adapter。
//!
//! 必须显式声明合同 §4.4 / 阶段 04 §6 的全部适配器字段。
//! 领域动作只通过实体状态邻接与仓储更新，不得 `$set` 绕过不变式。
//! 资金类 `PENDING_REVIEW` 已收敛为 `IN_APPROVAL`，不得再走通用状态更新。

mod customer_refund;
mod payment_reversal;
mod receipt_reversal;
mod supplier_refund;

pub use self::customer_refund::{
    CustomerRefundAdapter, build_customer_refund_snapshot, customer_refund_adapter,
    customer_refund_object_readable, customer_refund_responsible_org_id, customer_refund_start_command,
    customer_refund_subject_ref, execute_customer_refund_domain_action, require_frozen_binding,
    start_approval_command_kind,
};
pub use self::payment_reversal::{
    PaymentReversalAdapter, build_payment_reversal_snapshot, execute_payment_reversal_domain_action,
    payment_reversal_adapter, payment_reversal_object_readable, payment_reversal_responsible_org_id,
    payment_reversal_start_command, payment_reversal_start_command_kind, payment_reversal_subject_ref,
    require_payment_reversal_binding,
};
pub use self::receipt_reversal::{
    ReceiptReversalAdapter, build_receipt_reversal_snapshot, execute_receipt_reversal_domain_action,
    receipt_reversal_adapter, receipt_reversal_object_readable, receipt_reversal_responsible_org_id,
    receipt_reversal_start_command, receipt_reversal_start_command_kind, receipt_reversal_subject_ref,
    require_receipt_reversal_binding,
};
pub use self::supplier_refund::{
    SupplierRefundAdapter, build_supplier_refund_snapshot, execute_supplier_refund_domain_action,
    require_supplier_refund_binding, supplier_refund_adapter, supplier_refund_object_readable,
    supplier_refund_responsible_org_id, supplier_refund_start_command, supplier_refund_start_command_kind,
    supplier_refund_subject_ref,
};
