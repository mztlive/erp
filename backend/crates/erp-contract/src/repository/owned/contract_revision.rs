//! Owned MongoDB repository for [`crate::entity::contract::ContractRevision`].

/// Owned repository for `ContractRevision`.
///
/// Composes [`persistence_core::Repository`] and exposes only the read
/// primitives used by detail, list and cross-domain checks. Revisions are
/// immutable, so no update or delete surface is offered here.
pub struct ContractRevisionRepository<'a> {
    inner: persistence_core::Repository<'a, crate::entity::contract::ContractRevision>,
}

impl<'a> ContractRevisionRepository<'a> {
    /// Creates a `ContractRevision` repository bound to `collection_name`.
    ///
    /// # Parameters
    /// * `db` - MongoDB database handle
    /// * `collection_name` - collection name for `ContractRevision`
    ///
    /// # Returns
    /// Owned repository that delegates generic storage to persistence-core.
    pub fn new(db: &'a mongodb::Database, collection_name: &'a str) -> Self {
        Self { inner: persistence_core::Repository::new(db, collection_name) }
    }

    /// Returns the MongoDB database handle bound to this repository.
    ///
    /// # Returns
    /// The database handle used to construct this repository.
    pub fn database(&self) -> &'a mongodb::Database {
        self.inner.database()
    }

    /// Returns the MongoDB collection handle for `ContractRevision`.
    ///
    /// # Returns
    /// Typed collection handle for this repository's collection.
    pub fn collection(&self) -> mongodb::Collection<crate::entity::contract::ContractRevision> {
        self.inner.collection()
    }

    /// Finds an undeleted entity by id.
    ///
    /// # Parameters
    /// * `id` - stable entity id
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// MongoDB query failures.
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<crate::entity::contract::ContractRevision>> {
        self.inner.find_by_id(id, executor).await
    }

    /// Finds undeleted entities matching `filter`.
    ///
    /// # Parameters
    /// * `filter` - MongoDB filter document
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// MongoDB query or cursor failures.
    pub async fn find_many(
        &self,
        filter: mongodb::bson::Document,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<crate::entity::contract::ContractRevision>> {
        self.inner.find_many(filter, executor).await
    }

    /// Finds undeleted entities matching `filter`, sorted by `sort`.
    ///
    /// # Parameters
    /// * `filter` - MongoDB filter document
    /// * `sort` - MongoDB sort document
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// MongoDB query or cursor failures.
    pub async fn find_many_sorted(
        &self,
        filter: mongodb::bson::Document,
        sort: mongodb::bson::Document,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<crate::entity::contract::ContractRevision>> {
        self.inner.find_many_sorted(filter, sort, executor).await
    }
}
