//! 财务过账与付款申请的跨域业务编排。
mod attachment_upload;
pub use attachment_upload::{FinanceAttachmentService, FinanceAttachmentUpload};
mod errors;
mod ports;
pub use errors::{Error, Result};
pub use ports::{FinancePostingPorts, FinanceTaskPort};
pub mod finance_posting;
