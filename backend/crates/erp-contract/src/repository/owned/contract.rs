//! Owned MongoDB repository for [`crate::entity::contract::Contract`].

/// Owned repository for `Contract`.
///
/// Composes [`persistence_core::Repository`] and exposes only the primitives
/// actually used by domain writes and authorized reads. Specialized queries
/// stay in the original impl modules as inherent methods on this type.
pub struct ContractRepository<'a> {
    inner: persistence_core::Repository<'a, crate::entity::contract::Contract>,
}

impl<'a> ContractRepository<'a> {
    /// Creates a `Contract` repository bound to `collection_name`.
    ///
    /// # Parameters
    /// * `db` - MongoDB database handle
    /// * `collection_name` - collection name for `Contract`
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

    /// Returns the MongoDB collection handle for `Contract`.
    ///
    /// # Returns
    /// Typed collection handle for this repository's collection.
    pub fn collection(&self) -> mongodb::Collection<crate::entity::contract::Contract> {
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
    ) -> persistence_core::Result<Option<crate::entity::contract::Contract>> {
        self.inner.find_by_id(id, executor).await
    }

    /// Updates an entity with optimistic locking.
    ///
    /// # Parameters
    /// * `entity` - entity to persist
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// Version conflict or underlying write failures.
    pub async fn update(
        &self,
        entity: &mut crate::entity::contract::Contract,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<()>
    where
        crate::entity::contract::Contract: entity_core::HasBaseModel,
    {
        self.inner.update(entity, executor).await
    }

    /// Finds one undeleted entity matching `filter`.
    ///
    /// # Parameters
    /// * `filter` - MongoDB filter document
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// MongoDB query failures.
    pub async fn find_one(
        &self,
        filter: mongodb::bson::Document,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<crate::entity::contract::Contract>> {
        self.inner.find_one(filter, executor).await
    }
}
