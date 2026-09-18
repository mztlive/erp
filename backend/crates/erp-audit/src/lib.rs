//! Audit domain: audit log construction, persistence and command-receipt queries.

mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod repository;
mod service;

pub use dto::{AuditLogItem, AuditLogListParams};
pub use entity::{AuditLog, AuditLogData};
pub use error::{Error, Result};
pub use repository::{
    AuditExt, AuditLogFilter, AuditLogRepository, AuditLogRepositoryExt, SeparationAuditFact,
};
pub use service::{AuditActorLogs, AuditLogService, CommandReceipt, CommandReceiptServiceExt};
