//! 正式采购派生的供应商接口执行意图；敏感请求与协议上下文仅以密文保存。

mod model;
mod purchase_change;
mod repository;
mod setup_receipt;

pub use model::{ApiOrderExecution, ApiOrderSourceLine, ApiOrderStage};
pub use repository::{ApiOrderExt, active_for_purchase, due, ensure_indexes, for_purchase, reusable_address};
pub use setup_receipt::ApiOrderSetupReceipt;
