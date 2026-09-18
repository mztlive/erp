//! Owned MongoDB repository for [`crate::entity::legacy_import::LegacyImportRow`].

use std::ops::{Deref, DerefMut};

/// Owned repository for `LegacyImportRow`.
///
/// Composes [`persistence_core::Repository`] and dereferences to it so generic
/// CRUD and query methods need no per-entity forwarding. Specialized queries
/// stay in the original impl modules as inherent methods on this type.
pub struct LegacyImportRowRepository<'a> {
    inner: persistence_core::Repository<'a, crate::entity::legacy_import::LegacyImportRow>,
}

impl<'a> LegacyImportRowRepository<'a> {
    /// Creates a `LegacyImportRow` repository bound to `collection_name`.
    ///
    /// # 参数
    /// * `db` - MongoDB database handle
    /// * `collection_name` - collection name for `LegacyImportRow`
    ///
    /// # 返回
    /// Owned repository that delegates generic storage to persistence-core.
    ///
    /// # 错误
    /// 无。
    pub fn new(db: &'a mongodb::Database, collection_name: &'a str) -> Self {
        Self { inner: persistence_core::Repository::new(db, collection_name) }
    }
}

impl<'a> Deref for LegacyImportRowRepository<'a> {
    type Target = persistence_core::Repository<'a, crate::entity::legacy_import::LegacyImportRow>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for LegacyImportRowRepository<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
