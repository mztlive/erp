//! [`persistence_core::Repository`] 的集合范围别名。
//!
//! 领域方法是通用仓储上的扩展 trait。

pub type AuditLogRepository<'a> = persistence_core::Repository<'a, crate::entity::AuditLog>;
