//! 通用Repository实现
//!
//! 提供MongoDB数据库操作的通用接口，包括基础CRUD操作和各实体的特化方法

use crate::errors::{Error, Result};
use crate::mongo_ops;
use crate::Executor;
use entity_core::{BaseModel, HasBaseModel, NOT_DELETED_TIMESTAMP, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::{
    bson::{doc, serialize_to_document, Document},
    options::FindOptions,
    Database,
};
use serde::{de::DeserializeOwned, Serialize};

/// Defines filter behavior for database queries
///
/// This trait should be implemented by types that provide filtering criteria
/// for database queries.
pub trait QueryFilter {
    /// Converts the filter to a MongoDB document
    ///
    /// # Returns
    ///
    /// A MongoDB Document representing the filter criteria
    fn to_doc(&self) -> Document;
}

/// Defines pagination behavior for database queries
///
/// This trait should be implemented by types that provide pagination parameters
/// for database queries.
pub trait Pagination {
    /// Returns the requested page number and page size.
    ///
    /// Page numbers are one-based. Implementations may return `0`; the
    /// default offset calculation normalizes it to the first page.
    fn page_and_size(&self) -> (u64, u64);

    /// Returns number of items to skip
    ///
    /// # Returns
    ///
    /// The number of documents to skip in the result set
    fn skip(&self) -> u64 {
        let (page, page_size) = self.page_and_size();
        (page.max(1) - 1) * page_size
    }

    /// Returns maximum number of items to return
    ///
    /// # Returns
    ///
    /// The maximum number of documents to return
    fn limit(&self) -> i64 {
        let (_, page_size) = self.page_and_size();
        page_size as i64
    }
}

/// 分页结果集
#[derive(Debug, Serialize)]
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// 通用仓储结构体
pub struct Repository<'a, T> {
    db: &'a Database,
    collection_name: &'a str,
    _phantom: std::marker::PhantomData<T>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WriteMetadata {
    expected_version: i64,
    next_version: u64,
    next_version_bson: i64,
    updated_at: u64,
    updated_at_bson: i64,
}

/// 计算一次实体写入所需的版本与更新时间。
fn write_metadata(base: &BaseModel) -> Result<WriteMetadata> {
    write_metadata_at(base, chrono::Local::now().timestamp())
}

/// 使用指定时间计算实体写入元数据。
fn write_metadata_at(base: &BaseModel, updated_at_bson: i64) -> Result<WriteMetadata> {
    let next_version = base
        .version
        .checked_add(1)
        .ok_or(Error::EntityMetadataOutOfRange("version"))?;
    let expected_version =
        i64::try_from(base.version).map_err(|_| Error::EntityMetadataOutOfRange("version"))?;
    let next_version_bson =
        i64::try_from(next_version).map_err(|_| Error::EntityMetadataOutOfRange("version"))?;
    let updated_at =
        u64::try_from(updated_at_bson).map_err(|_| Error::EntityMetadataOutOfRange("updated_at"))?;

    Ok(WriteMetadata {
        expected_version,
        next_version,
        next_version_bson,
        updated_at,
        updated_at_bson,
    })
}

/// 构建活跃实体的乐观锁过滤条件。
fn active_cas_filter(base: &BaseModel, metadata: WriteMetadata) -> Document {
    doc! {
        "id": &base.id,
        "version": metadata.expected_version,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构建已删除实体的乐观锁过滤条件。
fn deleted_cas_filter(base: &BaseModel, metadata: WriteMetadata) -> Document {
    doc! {
        "id": &base.id,
        "version": metadata.expected_version,
        "deleted_at": { "$ne": NOT_DELETED_TIMESTAMP_BSON },
    }
}

/// 在 MongoDB 写入命中后同步内存中的持久化元数据。
fn apply_write_result(
    base: &mut BaseModel,
    metadata: WriteMetadata,
    deleted_at: Option<u64>,
    matched_count: u64,
) -> Result<()> {
    if matched_count == 0 {
        return Err(Error::OptimisticLockingError);
    }

    base.version = metadata.next_version;
    base.updated_at = metadata.updated_at;
    if let Some(deleted_at) = deleted_at {
        base.deleted_at = deleted_at;
    }
    Ok(())
}

impl<'a, T> Repository<'a, T>
where
    T: Serialize + DeserializeOwned + Send + Sync,
{
    /// 创建新的Repository实例
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `collection_name` - 集合名称
    ///
    /// # 返回
    /// 返回创建的实例。
    pub fn new(db: &'a Database, collection_name: &'a str) -> Self {
        Self {
            db,
            collection_name,
            _phantom: std::marker::PhantomData,
        }
    }

    /// 创建实体。
    ///
    /// # 参数
    /// * `entity` - 实体对象
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当唯一索引冲突或底层写入失败时返回错误。
    pub async fn create(&self, entity: &T, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), entity, executor).await
    }

    /// 根据ID查找未删除实体。
    ///
    /// # 参数
    /// * `id` - 标识符
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除实体；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<T>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 更新实体（带乐观锁）。
    ///
    /// # 参数
    /// * `entity` - 实体对象
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当实体已删除、版本冲突或底层写入失败时返回错误。
    pub async fn update(&self, entity: &mut T, executor: &mut dyn Executor) -> Result<()>
    where
        T: HasBaseModel,
    {
        if entity.base().is_deleted() {
            return Err(Error::OptimisticLockingError);
        }

        let metadata = write_metadata(entity.base())?;
        let filter = active_cas_filter(entity.base(), metadata);
        let mut document = serialize_to_document(&*entity)?;
        document.insert("version", metadata.next_version_bson);
        document.insert("updated_at", metadata.updated_at_bson);

        let result = mongo_ops::update_one(
            &self.collection(),
            filter,
            doc! { "$set": document },
            false,
            executor,
        )
        .await?;

        apply_write_result(entity.base_mut(), metadata, None, result.matched_count)
    }

    /// 软删除活跃实体。
    ///
    /// # 参数
    /// * `entity` - 待删除实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当实体已删除、版本冲突或底层写入失败时返回错误。
    pub async fn soft_delete(&self, entity: &mut T, executor: &mut dyn Executor) -> Result<()>
    where
        T: HasBaseModel,
    {
        if entity.base().is_deleted() {
            return Err(Error::OptimisticLockingError);
        }

        let metadata = write_metadata(entity.base())?;
        let filter = active_cas_filter(entity.base(), metadata);
        let result = mongo_ops::update_one(
            &self.collection(),
            filter,
            doc! {
                "$set": {
                    "version": metadata.next_version_bson,
                    "updated_at": metadata.updated_at_bson,
                    "deleted_at": metadata.updated_at_bson,
                }
            },
            false,
            executor,
        )
        .await?;

        apply_write_result(
            entity.base_mut(),
            metadata,
            Some(metadata.updated_at),
            result.matched_count,
        )
    }

    /// 恢复已软删除实体。
    ///
    /// # 参数
    /// * `entity` - 待恢复实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当实体未删除、版本冲突或底层写入失败时返回错误。
    pub async fn restore(&self, entity: &mut T, executor: &mut dyn Executor) -> Result<()>
    where
        T: HasBaseModel,
    {
        if !entity.base().is_deleted() {
            return Err(Error::OptimisticLockingError);
        }

        let metadata = write_metadata(entity.base())?;
        let filter = deleted_cas_filter(entity.base(), metadata);
        let result = mongo_ops::update_one(
            &self.collection(),
            filter,
            doc! {
                "$set": {
                    "version": metadata.next_version_bson,
                    "updated_at": metadata.updated_at_bson,
                    "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                }
            },
            false,
            executor,
        )
        .await?;

        apply_write_result(
            entity.base_mut(),
            metadata,
            Some(NOT_DELETED_TIMESTAMP),
            result.matched_count,
        )
    }

    /// 查找所有未删除的实体。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部未删除实体。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_all(&self, executor: &mut dyn Executor) -> Result<Vec<T>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 根据单个字段查找一个未删除实体。
    ///
    /// # 参数
    /// * `field` - 字段名
    /// * `value` - 值
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除实体；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub(crate) async fn find_one_by_field<V>(
        &self,
        field: &str,
        value: V,
        executor: &mut dyn Executor,
    ) -> Result<Option<T>>
    where
        V: Into<mongodb::bson::Bson> + Send,
    {
        let filter = doc! { field: value.into(), "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        mongo_ops::find_one(&self.collection(), filter, executor).await
    }

    /// 查找单个未删除实体。
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除实体；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub(crate) async fn find_one(&self, filter: Document, executor: &mut dyn Executor) -> Result<Option<T>> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);

        mongo_ops::find_one(&self.collection(), filter, executor).await
    }

    /// 查找多个未删除实体。
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回符合条件且未删除的实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub(crate) async fn find_many(&self, filter: Document, executor: &mut dyn Executor) -> Result<Vec<T>> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);

        mongo_ops::find_many(&self.collection(), filter, FindOptions::default(), executor).await
    }

    /// 查找多个未删除实体（带排序）。
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `sort` - 排序条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回排序后的未删除实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub(crate) async fn find_many_sorted(
        &self,
        filter: Document,
        sort: Document,
        executor: &mut dyn Executor,
    ) -> Result<Vec<T>> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);

        mongo_ops::find_many(
            &self.collection(),
            filter,
            FindOptions::builder().sort(sort).build(),
            executor,
        )
        .await
    }

    /// 判断是否存在符合条件的活跃实体。
    ///
    /// 查询只投影 MongoDB `_id`，并在首条命中后停止，
    /// 避免为存在性判断加载完整实体或结果集合。
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 存在匹配实体时返回 `true`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub(crate) async fn exists(&self, filter: Document, executor: &mut dyn Executor) -> Result<bool> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);

        mongo_ops::exists(
            &self.db.collection::<Document>(self.collection_name),
            filter,
            executor,
        )
        .await
    }

    /// 分页检索实体。
    ///
    /// # 参数
    /// * `filter` - 过滤与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回当前页实体与匹配总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub(crate) async fn search<F>(&self, filter: &F, executor: &mut dyn Executor) -> Result<PageResult<T>>
    where
        F: QueryFilter + Pagination + Send + Sync,
    {
        let items = mongo_ops::find_many(
            &self.collection(),
            filter.to_doc(),
            FindOptions::builder()
                .sort(doc! { "created_at": -1 })
                .skip(filter.skip())
                .limit(filter.limit())
                .build(),
            &mut *executor,
        )
        .await?;
        let total = self.search_count(filter, executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 统计符合条件的实体总数。
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回匹配实体总数。
    ///
    /// # 错误
    /// 当 MongoDB 统计失败时返回错误。
    async fn search_count<F>(&self, filter: &F, executor: &mut dyn Executor) -> Result<u64>
    where
        F: QueryFilter + Send + Sync,
    {
        mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await
    }

    /// 获取当前实体对应的 MongoDB 集合（内部使用）。
    ///
    /// # 返回
    /// 返回按实体类型参数化的集合句柄。
    pub(crate) fn collection(&self) -> mongodb::Collection<T> {
        self.db.collection::<T>(self.collection_name)
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_write_result, write_metadata_at, Pagination};
    use crate::errors::Error;
    use entity_core::BaseModel;

    struct TestPagination {
        page: u64,
        page_size: u64,
    }

    impl Pagination for TestPagination {
        fn page_and_size(&self) -> (u64, u64) {
            (self.page, self.page_size)
        }
    }

    #[test]
    fn pagination_normalizes_zero_to_first_page() {
        let pagination = TestPagination {
            page: 0,
            page_size: 20,
        };

        assert_eq!(pagination.skip(), 0);
        assert_eq!(pagination.limit(), 20);
    }

    #[test]
    fn pagination_calculates_offset_for_regular_page() {
        let pagination = TestPagination {
            page: 3,
            page_size: 20,
        };

        assert_eq!(pagination.skip(), 40);
        assert_eq!(pagination.limit(), 20);
    }

    #[test]
    fn write_metadata_increments_version_and_uses_given_timestamp() {
        let mut base = BaseModel::new("entity_1".to_string());
        base.version = 7;

        let metadata = write_metadata_at(&base, 1_700_000_000).expect("metadata should be valid");

        assert_eq!(metadata.expected_version, 7);
        assert_eq!(metadata.next_version, 8);
        assert_eq!(metadata.next_version_bson, 8);
        assert_eq!(metadata.updated_at, 1_700_000_000);
    }

    #[test]
    fn write_metadata_rejects_version_overflow() {
        let mut base = BaseModel::new("entity_1".to_string());
        base.version = u64::MAX;

        let error = write_metadata_at(&base, 1_700_000_000).expect_err("overflow should fail");

        assert!(matches!(error, Error::EntityMetadataOutOfRange("version")));
    }

    #[test]
    fn write_metadata_rejects_version_outside_bson_range() {
        let mut base = BaseModel::new("entity_1".to_string());
        base.version = i64::MAX as u64;

        let error = write_metadata_at(&base, 1_700_000_000).expect_err("BSON overflow should fail");

        assert!(matches!(error, Error::EntityMetadataOutOfRange("version")));
    }

    #[test]
    fn failed_cas_does_not_change_in_memory_metadata() {
        let mut base = BaseModel::new("entity_1".to_string());
        let original = base.clone();
        let metadata = write_metadata_at(&base, 1_700_000_000).expect("metadata should be valid");

        let error = apply_write_result(&mut base, metadata, Some(123), 0).expect_err("CAS should fail");

        assert!(matches!(error, Error::OptimisticLockingError));
        assert_eq!(base, original);
    }

    #[test]
    fn successful_write_synchronizes_in_memory_metadata() {
        let mut base = BaseModel::new("entity_1".to_string());
        let metadata = write_metadata_at(&base, 1_700_000_000).expect("metadata should be valid");

        apply_write_result(&mut base, metadata, Some(123), 1).expect("CAS should succeed");

        assert_eq!(base.version, metadata.next_version);
        assert_eq!(base.updated_at, metadata.updated_at);
        assert_eq!(base.deleted_at, 123);
    }
}
