//! 审计领域：安全业务事件、独立尝试与授权展示查询。

mod catalog;
mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod repository;
mod service;

pub use catalog::{registered_action, registered_actions};
pub use dto::{AuditLogItem, AuditLogListParams};
pub use entity::{
    AuditAction, AuditAttempt, AuditAttemptKind, AuditAttemptResult, AuditCode, AuditFact, AuditField,
    AuditFieldChange, AuditFieldKind, AuditLog, AuditLogData, AuditValue, BusinessAuditEvent,
    BusinessAuditFact, BusinessAuditFieldChange, BusinessEventContent, BusinessEventContext,
    BusinessEventResult,
};
pub use error::{Error, Result};
pub use repository::{AuditAttemptExt, AuditExt, AuditLogFilter, AuditLogRepository, AuditLogRepositoryExt};
pub use service::{AuditActorLogs, AuditLogService, attempt_context, prepare_business_log};
