//! 财务集合名与拥有仓储工厂的唯一合同。

mod command_receipt;
mod cost;
mod payable;
mod receivable;

pub use command_receipt::FinanceCommandExt;
pub use cost::CostExt;
pub use payable::PayableExt;
pub use receivable::ReceivableExt;
