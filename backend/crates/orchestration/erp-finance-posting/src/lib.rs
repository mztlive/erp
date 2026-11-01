//! 财务过账与付款申请的跨域业务编排。
mod errors;
mod ports;
pub use errors::{Error, Result};
pub use ports::{FinancePostingPorts, FinanceTaskPort};
pub mod attachments;
pub mod finance_posting;
pub use attachments::commit_supplier_payment_with_assets;
