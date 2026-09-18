//! Owned catalog repositories composed from persistence-core.
//!
//! The twelve single-collection repositories below are pure delegation over
//! [`persistence_core::Repository`] (identical method sets, only the entity
//! type differs); they are stamped by [`define_owned_repository`] instead of
//! twelve copied files. Specialized queries stay in `repository::catalog` as
//! inherent methods on these types.

/// Stamps one owned repository delegating to [`persistence_core::Repository`].
///
/// Generated type keeps the same public name, method set and signatures as the
/// former hand-written forwarding file; only the entity type varies.
macro_rules! define_owned_repository {
    ($(#[$meta:meta])* $repository:ident, $entity:ty) => {
        $(#[$meta])*
        pub struct $repository<'a> {
            inner: persistence_core::Repository<'a, $entity>,
        }

        impl<'a> $repository<'a> {
            /// Creates a repository bound to `collection_name`.
            ///
            /// # Parameters
            /// * `db` - MongoDB database handle
            /// * `collection_name` - collection name for the entity
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

            /// Returns the MongoDB collection handle for the entity.
            ///
            /// # Returns
            /// Typed collection handle for this repository's collection.
            pub fn collection(&self) -> mongodb::Collection<$entity> {
                self.inner.collection()
            }

            /// Creates an entity.
            ///
            /// # Parameters
            /// * `entity` - entity to insert
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # Errors
            /// Duplicate-key or underlying write failures.
            pub async fn create(
                &self,
                entity: &$entity,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<()> {
                self.inner.create(entity, executor).await
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
            ) -> persistence_core::Result<Option<$entity>> {
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
                entity: &mut $entity,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<()>
            where
                $entity: entity_core::HasBaseModel,
            {
                self.inner.update(entity, executor).await
            }

            /// Soft-deletes an active entity.
            ///
            /// # Parameters
            /// * `entity` - entity to delete
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # Errors
            /// Version conflict or underlying write failures.
            pub async fn soft_delete(
                &self,
                entity: &mut $entity,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<()>
            where
                $entity: entity_core::HasBaseModel,
            {
                self.inner.soft_delete(entity, executor).await
            }

            /// Restores a soft-deleted entity.
            ///
            /// # Parameters
            /// * `entity` - entity to restore
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # Errors
            /// Version conflict or underlying write failures.
            pub async fn restore(
                &self,
                entity: &mut $entity,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<()>
            where
                $entity: entity_core::HasBaseModel,
            {
                self.inner.restore(entity, executor).await
            }

            /// Lists all undeleted entities.
            ///
            /// # Parameters
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # Errors
            /// MongoDB query or cursor failures.
            pub async fn list_all(
                &self,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.list_all(executor).await
            }

            /// Finds one undeleted entity by a single field.
            ///
            /// # Parameters
            /// * `field` - field name
            /// * `value` - field value
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # Errors
            /// MongoDB query failures.
            pub async fn find_one_by_field<V>(
                &self,
                field: &str,
                value: V,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Option<$entity>>
            where
                V: Into<mongodb::bson::Bson> + Send,
            {
                self.inner.find_one_by_field(field, value, executor).await
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
            ) -> persistence_core::Result<Option<$entity>> {
                self.inner.find_one(filter, executor).await
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
            ) -> persistence_core::Result<Vec<$entity>> {
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
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.find_many_sorted(filter, sort, executor).await
            }

            /// Loads undeleted entities by stable ids.
            ///
            /// Generic by-id projection primitive; not bound to WorkItem.
            ///
            /// # Parameters
            /// * `ids` - stable ids
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # Errors
            /// MongoDB query or deserialization failures.
            pub async fn list_active_by_ids(
                &self,
                ids: &[String],
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.list_active_by_ids(ids, executor).await
            }

            /// Returns whether an active entity matching `filter` exists.
            ///
            /// # Parameters
            /// * `filter` - MongoDB filter document
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # Errors
            /// MongoDB query failures.
            pub async fn exists(
                &self,
                filter: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<bool> {
                self.inner.exists(filter, executor).await
            }

            /// Pages entities matching `filter`.
            ///
            /// # Parameters
            /// * `filter` - filter and pagination
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # Errors
            /// MongoDB query, cursor, or count failures.
            pub async fn search<F>(
                &self,
                filter: &F,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<persistence_core::PageResult<$entity>>
            where
                F: persistence_core::QueryFilter + persistence_core::Pagination + Send + Sync,
            {
                self.inner.search(filter, executor).await
            }
        }
    };
}

define_owned_repository!(
    /// Owned repository for `Product`.
    ProductRepository,
    crate::entity::catalog::Product
);
define_owned_repository!(
    /// Owned repository for `ProductBrand`.
    ProductBrandRepository,
    crate::entity::catalog::ProductBrand
);
define_owned_repository!(
    /// Owned repository for `ProductCategory`.
    ProductCategoryRepository,
    crate::entity::catalog::ProductCategory
);
define_owned_repository!(
    /// Owned repository for `ProductCategoryAttribute`.
    ProductCategoryAttributeRepository,
    crate::entity::catalog::ProductCategoryAttribute
);
define_owned_repository!(
    /// Owned repository for `ProductRevision`.
    ProductRevisionRepository,
    crate::entity::catalog::ProductRevision
);
define_owned_repository!(
    /// Owned repository for `ProductRevisionMedia`.
    ProductRevisionMediaRepository,
    crate::entity::catalog::ProductRevisionMedia
);
define_owned_repository!(
    /// Owned repository for `Sku`.
    SkuRepository,
    crate::entity::catalog::Sku
);
define_owned_repository!(
    /// Owned repository for `SkuAttribute`.
    SkuAttributeRepository,
    crate::entity::catalog::SkuAttribute
);
define_owned_repository!(
    /// Owned repository for `SkuAttributeValue`.
    SkuAttributeValueRepository,
    crate::entity::catalog::SkuAttributeValue
);
define_owned_repository!(
    /// Owned repository for `SkuRevision`.
    SkuRevisionRepository,
    crate::entity::catalog::SkuRevision
);
define_owned_repository!(
    /// Owned repository for `UnitOfMeasure`.
    UnitOfMeasureRepository,
    crate::entity::catalog::UnitOfMeasure
);
define_owned_repository!(
    /// Owned repository for `VoucherCategoryProfileRevision`.
    VoucherCategoryProfileRevisionRepository,
    crate::entity::catalog::VoucherCategoryProfileRevision
);
