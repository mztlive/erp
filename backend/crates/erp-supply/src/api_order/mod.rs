//! 正式采购派生的供应商接口执行意图；敏感请求与协议上下文仅以密文保存。

mod cancellation;
mod confirmation;
mod funds_resolution;
mod model;
mod purchase_change;
mod receipt;
mod repository;
mod setup_receipt;

pub use cancellation::{CancellationAction, CancellationActionState, CancellationScope};
pub use confirmation::{ConfirmationAction, ConfirmationActionState};
pub use funds_resolution::ExecutionFundsFinality;
pub use model::{ApiOrderExecution, ApiOrderSourceLine, ApiOrderStage};
pub use receipt::{ReceiptAction, ReceiptActionState};
pub use repository::{
    ApiOrderExt, active_for_purchase, connections_by_ids, due, ensure_indexes, for_external_order,
    for_purchase, for_sales_lines, fulfillment_orders_by_execution_ids, receipts_due, reusable_address,
};
pub use setup_receipt::ApiOrderSetupReceipt;
