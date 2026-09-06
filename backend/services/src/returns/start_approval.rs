//! 客户退款提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。

mod customer_refund;
mod mapping;
mod payment_reversal;
mod prepare;
mod receipt_reversal;
mod supplier_refund;

pub(super) use customer_refund::{
    build_customer_refund_start_input, load_start_receipt, persist_customer_refund_start,
    persist_runtime_writes, CustomerRefundStartInput, CustomerRefundStartPersistInput,
};
pub(super) use payment_reversal::{
    build_payment_reversal_start_input, load_payment_reversal_start_receipt,
    persist_payment_reversal_runtime, persist_payment_reversal_start, PaymentReversalStartInput,
    PaymentReversalStartPersistInput,
};
pub(super) use prepare::{
    ensure_return_start_actor_active, ensure_return_start_replay_authorized, load_bound_definition_graph,
    load_bound_definition_graph_with_executor, replay_return_start_with_executor, replay_subject_versions,
    ReplayReturnStartInput,
};
pub(super) use receipt_reversal::{
    build_receipt_reversal_start_input, load_receipt_reversal_start_receipt,
    persist_receipt_reversal_runtime, persist_receipt_reversal_start, ReceiptReversalStartInput,
    ReceiptReversalStartPersistInput,
};
pub(super) use supplier_refund::{
    build_supplier_refund_start_input, load_supplier_refund_start_receipt, persist_supplier_refund_runtime,
    persist_supplier_refund_start, SupplierRefundStartInput, SupplierRefundStartPersistInput,
};
