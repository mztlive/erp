//! Owned MongoDB repository for [`crate::entity::purchase_order::PurchaseOrder`].

use std::ops::{Deref, DerefMut};

/// Owned repository for `PurchaseOrder`.
///
/// Composes [`persistence_core::Repository`] and dereferences to it so generic
/// CRUD and query methods need no per-entity forwarding. Specialized queries
/// stay in the original impl modules as inherent methods on this type.
pub struct PurchaseOrderRepository<'a> {
    inner: persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseOrder>,
}

impl<'a> PurchaseOrderRepository<'a> {
    /// Creates a `PurchaseOrder` repository bound to `collection_name`.
    ///
    /// # 参数
    /// * `db` - MongoDB database handle
    /// * `collection_name` - collection name for `PurchaseOrder`
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

impl<'a> Deref for PurchaseOrderRepository<'a> {
    type Target = persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseOrder>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for PurchaseOrderRepository<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
