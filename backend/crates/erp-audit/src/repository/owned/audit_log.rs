//! Owned MongoDB repository for [`crate::entity::AuditLog`].

use std::ops::Deref;

/// Owned repository for `AuditLog`.
///
/// Composes [`persistence_core::Repository`] and dereferences to it, so generic
/// CRUD and query primitives resolve without per-method forwarding.
/// Specialized queries stay in the original impl modules as inherent methods
/// on this type (erp-audit-010).
pub struct AuditLogRepository<'a> {
    inner: persistence_core::Repository<'a, crate::entity::AuditLog>,
}

impl<'a> AuditLogRepository<'a> {
    /// Creates a `AuditLog` repository bound to `collection_name`.
    ///
    /// # 参数
    /// * `db` - MongoDB database handle
    /// * `collection_name` - collection name for `AuditLog`
    ///
    /// # 返回
    /// Owned repository that delegates generic storage to persistence-core.
    pub fn new(db: &'a mongodb::Database, collection_name: &'a str) -> Self {
        Self { inner: persistence_core::Repository::new(db, collection_name) }
    }
}

impl<'a> Deref for AuditLogRepository<'a> {
    type Target = persistence_core::Repository<'a, crate::entity::AuditLog>;

    /// Returns the composed generic repository.
    ///
    /// # 参数
    /// * `self` - 审计日志 owned 仓储
    ///
    /// # 返回
    /// 返回 persistence-core 泛型仓储引用，通用 CRUD 经它解析。
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
