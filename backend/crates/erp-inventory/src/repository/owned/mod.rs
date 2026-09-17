//! Owned inventory repositories composed from persistence-core.

/// 生成 owned 仓储透调层的声明宏（五集合结构同构，仅实体类型不同）。
///
/// 生成的仓储组合 [`persistence_core::Repository`] 并暴露调用方所需的 CRUD
/// 与查询方法；特化查询仍在各集合的原有 impl 模块中作为本类型的固有方法。
macro_rules! define_owned_repository {
    ($repository:ident, $entity:ty) => {
        /// Owned repository composed from persistence-core.
        ///
        /// Specialized queries stay in the original impl modules as inherent
        /// methods on this type.
        pub struct $repository<'a> {
            inner: persistence_core::Repository<'a, $entity>,
        }

        impl<'a> $repository<'a> {
            /// Creates a repository bound to `collection_name`.
            ///
            /// # 参数
            /// * `db` - MongoDB database handle
            /// * `collection_name` - collection name for the owned entity
            ///
            /// # 返回
            /// 返回委托通用存储的 owned 仓储。
            pub fn new(db: &'a mongodb::Database, collection_name: &'a str) -> Self {
                Self { inner: persistence_core::Repository::new(db, collection_name) }
            }

            /// Returns the MongoDB database handle bound to this repository.
            ///
            /// # 返回
            /// 返回构造时绑定的数据库句柄。
            pub fn database(&self) -> &'a mongodb::Database {
                self.inner.database()
            }

            /// Returns the MongoDB collection handle for the owned entity.
            ///
            /// # 返回
            /// 返回本仓储集合的类型化句柄。
            pub fn collection(&self) -> mongodb::Collection<$entity> {
                self.inner.collection()
            }

            /// Creates an entity.
            ///
            /// # 参数
            /// * `entity` - entity to insert
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// 唯一键冲突或底层写入失败。
            pub async fn create(
                &self,
                entity: &$entity,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<()> {
                self.inner.create(entity, executor).await
            }

            /// Finds an undeleted entity by id.
            ///
            /// # 参数
            /// * `id` - stable entity id
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// MongoDB 查询失败。
            pub async fn find_by_id(
                &self,
                id: &str,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Option<$entity>> {
                self.inner.find_by_id(id, executor).await
            }

            /// Updates an entity with optimistic locking.
            ///
            /// # 参数
            /// * `entity` - entity to persist
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// 版本冲突或底层写入失败。
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
            /// # 参数
            /// * `entity` - entity to delete
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// 版本冲突或底层写入失败。
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
            /// # 参数
            /// * `entity` - entity to restore
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// 版本冲突或底层写入失败。
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
            /// # 参数
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// MongoDB 查询或游标失败。
            pub async fn list_all(
                &self,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.list_all(executor).await
            }

            /// Finds one undeleted entity by a single field.
            ///
            /// # 参数
            /// * `field` - field name
            /// * `value` - field value
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// MongoDB 查询失败。
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
            /// # 参数
            /// * `filter` - MongoDB filter document
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// MongoDB 查询失败。
            pub async fn find_one(
                &self,
                filter: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Option<$entity>> {
                self.inner.find_one(filter, executor).await
            }

            /// Finds undeleted entities matching `filter`.
            ///
            /// # 参数
            /// * `filter` - MongoDB filter document
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// MongoDB 查询或游标失败。
            pub async fn find_many(
                &self,
                filter: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.find_many(filter, executor).await
            }

            /// Finds undeleted entities matching `filter`, sorted by `sort`.
            ///
            /// # 参数
            /// * `filter` - MongoDB filter document
            /// * `sort` - MongoDB sort document
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// MongoDB 查询或游标失败。
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
            /// # 参数
            /// * `ids` - stable ids
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// MongoDB 查询或反序列化失败。
            pub async fn list_active_by_ids(
                &self,
                ids: &[String],
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.list_active_by_ids(ids, executor).await
            }

            /// Returns whether an active entity matching `filter` exists.
            ///
            /// # 参数
            /// * `filter` - MongoDB filter document
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// MongoDB 查询失败。
            pub async fn exists(
                &self,
                filter: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<bool> {
                self.inner.exists(filter, executor).await
            }

            /// Pages entities matching `filter`.
            ///
            /// # 参数
            /// * `filter` - filter and pagination
            /// * `executor` - data-access executor chosen by the caller
            ///
            /// # 错误
            /// MongoDB 查询、游标或计数失败。
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

pub(crate) use define_owned_repository;

mod stock_adjustment;
mod stock_adjustment_line;
mod stock_balance;
mod stock_movement;
mod stock_reservation;

pub use stock_adjustment::StockAdjustmentRepository;
pub use stock_adjustment_line::StockAdjustmentLineRepository;
pub use stock_balance::StockBalanceRepository;
pub use stock_movement::StockMovementRepository;
pub use stock_reservation::StockReservationRepository;
