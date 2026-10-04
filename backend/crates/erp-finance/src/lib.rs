//! Financial accounts, invoices, payments and cost facts.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use entity::command_receipt::{FinanceCommandReceipt, FinanceCommandResource, FinanceCommandResult};
pub use error::{Error, Result};
pub use repository::{FinanceCommandExt, FinanceCommandReceiptRepositoryExt};
pub use service::command_receipt::FinanceCommandReceiptService;
