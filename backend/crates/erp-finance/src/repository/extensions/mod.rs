//! 财务集合名与拥有仓储工厂的唯一合同。

mod cost;
mod payable;
mod receivable;

pub use cost::CostExt;
pub use payable::PayableExt;
pub use receivable::ReceivableExt;
