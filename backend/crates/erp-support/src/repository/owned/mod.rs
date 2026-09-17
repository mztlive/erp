//! Owned support repositories composed from persistence-core.

/// Owned 仓储委托样板的统一声明宏。
///
/// 九个 owned 仓储文件此前为逐字重复结构，仅实体类型与公开类型名不同；
/// 宏生成结构体与全部委托方法，各文件只保留一行调用。特异查询仍留在原
/// impl 模块作 inherent 方法。对外类型名与方法签名不变。
macro_rules! owned_repo {
    ($repo:ident, $entity:ty) => {
        /// 由 persistence-core 组合而成的 owned 仓储薄包装。
        ///
        /// 通用 CRUD 与查询委托给内部存储；特异查询在原 impl 模块作
        /// inherent 方法保留。
        pub struct $repo<'a> {
            inner: persistence_core::Repository<'a, $entity>,
        }

        impl<'a> $repo<'a> {
            /// 创建绑定到 `collection_name` 的 owned 仓储。
            ///
            /// # 参数
            /// * `db` - MongoDB 数据库句柄
            /// * `collection_name` - 实体集合名
            ///
            /// # 返回
            /// 委托通用存储的 owned 仓储。
            pub fn new(db: &'a mongodb::Database, collection_name: &'a str) -> Self {
                Self { inner: persistence_core::Repository::new(db, collection_name) }
            }

            /// 返回绑定到本仓储的 MongoDB 数据库句柄。
            ///
            /// # 返回
            /// 构造时传入的数据库句柄。
            pub fn database(&self) -> &'a mongodb::Database {
                self.inner.database()
            }

            /// 返回本仓储实体的 MongoDB 集合句柄。
            ///
            /// # 返回
            /// 本仓储集合的类型化集合句柄。
            pub fn collection(&self) -> mongodb::Collection<$entity> {
                self.inner.collection()
            }

            /// 创建实体。
            ///
            /// # 参数
            /// * `entity` - 待插入实体
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 错误
            /// 唯一键冲突或底层写入失败时返回错误。
            pub async fn create(
                &self,
                entity: &$entity,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<()> {
                self.inner.create(entity, executor).await
            }

            /// 按 ID 查找未删除实体。
            ///
            /// # 参数
            /// * `id` - 稳定实体 ID
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 返回
            /// 命中时返回实体，未命中返回 `None`。
            ///
            /// # 错误
            /// MongoDB 查询失败时返回错误。
            pub async fn find_by_id(
                &self,
                id: &str,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Option<$entity>> {
                self.inner.find_by_id(id, executor).await
            }

            /// 带乐观锁更新实体。
            ///
            /// # 参数
            /// * `entity` - 待持久化实体
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 错误
            /// 版本冲突或底层写入失败时返回错误。
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

            /// 软删除活跃实体。
            ///
            /// # 参数
            /// * `entity` - 待删除实体
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 错误
            /// 版本冲突或底层写入失败时返回错误。
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

            /// 恢复被软删除的实体。
            ///
            /// # 参数
            /// * `entity` - 待恢复实体
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 错误
            /// 版本冲突或底层写入失败时返回错误。
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

            /// 列出全部未删除实体。
            ///
            /// # 参数
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 返回
            /// 全部未删除实体。
            ///
            /// # 错误
            /// MongoDB 查询或游标读取失败时返回错误。
            pub async fn list_all(
                &self,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.list_all(executor).await
            }

            /// 按单个字段查找一个未删除实体。
            ///
            /// # 参数
            /// * `field` - 字段名
            /// * `value` - 字段值
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 返回
            /// 命中时返回实体，未命中返回 `None`。
            ///
            /// # 错误
            /// MongoDB 查询失败时返回错误。
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

            /// 查找一个匹配 `filter` 的未删除实体。
            ///
            /// # 参数
            /// * `filter` - MongoDB 过滤文档
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 返回
            /// 命中时返回实体，未命中返回 `None`。
            ///
            /// # 错误
            /// MongoDB 查询失败时返回错误。
            pub async fn find_one(
                &self,
                filter: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Option<$entity>> {
                self.inner.find_one(filter, executor).await
            }

            /// 查找匹配 `filter` 的未删除实体集合。
            ///
            /// # 参数
            /// * `filter` - MongoDB 过滤文档
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 返回
            /// 匹配的未删除实体集合。
            ///
            /// # 错误
            /// MongoDB 查询或游标读取失败时返回错误。
            pub async fn find_many(
                &self,
                filter: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.find_many(filter, executor).await
            }

            /// 查找匹配 `filter` 的未删除实体集合并按 `sort` 排序。
            ///
            /// # 参数
            /// * `filter` - MongoDB 过滤文档
            /// * `sort` - MongoDB 排序文档
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 返回
            /// 匹配且已排序的未删除实体集合。
            ///
            /// # 错误
            /// MongoDB 查询或游标读取失败时返回错误。
            pub async fn find_many_sorted(
                &self,
                filter: mongodb::bson::Document,
                sort: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.find_many_sorted(filter, sort, executor).await
            }

            /// 按稳定 ID 批量加载未删除实体。
            ///
            /// # 参数
            /// * `ids` - 稳定 ID 集合
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 返回
            /// 命中 ID 的未删除实体集合。
            ///
            /// # 错误
            /// MongoDB 查询或反序列化失败时返回错误。
            pub async fn list_active_by_ids(
                &self,
                ids: &[String],
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<Vec<$entity>> {
                self.inner.list_active_by_ids(ids, executor).await
            }

            /// 判断是否存在匹配 `filter` 的活跃实体。
            ///
            /// # 参数
            /// * `filter` - MongoDB 过滤文档
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 返回
            /// 存在匹配时返回 `true`。
            ///
            /// # 错误
            /// MongoDB 查询失败时返回错误。
            pub async fn exists(
                &self,
                filter: mongodb::bson::Document,
                executor: &mut dyn persistence_core::Executor,
            ) -> persistence_core::Result<bool> {
                self.inner.exists(filter, executor).await
            }

            /// 分页查询匹配 `filter` 的实体。
            ///
            /// # 参数
            /// * `filter` - 过滤与分页条件
            /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
            ///
            /// # 返回
            /// 当前页实体与总数。
            ///
            /// # 错误
            /// MongoDB 查询、游标读取或计数失败时返回错误。
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

pub(crate) use owned_repo;

mod background_job;
mod background_job_item;
mod bulk_selection_item;
mod bulk_selection_snapshot;
mod document_attachment;
mod external_identity_map;
mod external_identity_target;
mod file_asset;
mod source_system;

pub use background_job::BackgroundJobRepository;
pub use background_job_item::BackgroundJobItemRepository;
pub use bulk_selection_item::BulkSelectionItemRepository;
pub use bulk_selection_snapshot::BulkSelectionSnapshotRepository;
pub use document_attachment::DocumentAttachmentRepository;
pub use external_identity_map::ExternalIdentityMapRepository;
pub use external_identity_target::ExternalIdentityTargetRepository;
pub use file_asset::FileAssetRepository;
pub use source_system::SourceSystemRepository;
