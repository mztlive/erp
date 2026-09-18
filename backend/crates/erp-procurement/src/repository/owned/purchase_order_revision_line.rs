//! Owned MongoDB repository for [`crate::entity::purchase_order::PurchaseOrderRevisionLine`].

use std::ops::{Deref, DerefMut};

/// Owned repository for `PurchaseOrderRevisionLine`.
///
/// Composes [`persistence_core::Repository`] and dereferences to it so generic
/// CRUD and query methods need no per-entity forwarding. Specialized queries
/// stay in the original impl modules as inherent methods on this type.
pub struct PurchaseOrderRevisionLineRepository<'a> {
    inner: persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseOrderRevisionLine>,
}

impl<'a> PurchaseOrderRevisionLineRepository<'a> {
    /// Creates a `PurchaseOrderRevisionLine` repository bound to `collection_name`.
    ///
    /// # 参数
    /// * `db` - MongoDB database handle
    /// * `collection_name` - collection name for `PurchaseOrderRevisionLine`
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

impl<'a> Deref for PurchaseOrderRevisionLineRepository<'a> {
    type Target = persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseOrderRevisionLine>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for PurchaseOrderRevisionLineRepository<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
