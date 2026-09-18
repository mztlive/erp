//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type AuditLogRepository<'a> = persistence_core::Repository<'a, crate::entity::AuditLog>;
