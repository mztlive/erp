//! 通用Repository实现
//!
//! 提供MongoDB数据库操作的通用接口，包括基础CRUD操作和各实体的特化方法

use crate::errors::{Error, Result};
use entity_core::{BaseModel, HasBaseModel, NOT_DELETED_TIMESTAMP, NOT_DELETED_TIMESTAMP_BSON};
use futures_util::StreamExt;
use mongodb::{
    bson::{doc, to_document, Document},
    ClientSession, Cursor, Database,
};
use serde::{de::DeserializeOwned, Serialize};

/// Converts a MongoDB cursor into a vector of items
///
/// # Type Parameters
///
/// * `T` - The type of items to collect from the cursor
///
/// # Arguments
///
/// * `cursor` - MongoDB cursor to convert
///
/// # Returns
///
/// Returns a Result containing a Vec of items or an error
async fn cursor_to_vec<T>(mut cursor: Cursor<T>) -> Result<Vec<T>>
where
    Cursor<T>: futures_util::stream::StreamExt<Item = std::result::Result<T, mongodb::error::Error>>,
{
    let mut result = vec![];
    while let Some(item) = cursor.next().await {
        result.push(item?);
    }

    Ok(result)
}

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

    /// 创建实体
    ///
    /// # 参数
    /// * `entity` - 实体对象
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn create(&self, entity: &T) -> Result<()> {
        self.db
            .collection::<T>(self.collection_name)
            .insert_one(entity)
            .await?;
        Ok(())
    }

    /// 创建实体（带事务）
    ///
    /// # 参数
    /// * `entity` - 实体对象
    /// * `session` - 事务会话
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn create_with_session(&self, entity: &T, session: &mut ClientSession) -> Result<()> {
        self.db
            .collection::<T>(self.collection_name)
            .insert_one(entity)
            .session(session)
            .await?;
        Ok(())
    }

    /// 根据ID查找
    ///
    /// # 参数
    /// * `id` - 标识符
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当内部逻辑或依赖操作失败时返回错误。
    pub async fn find_by_id(&self, id: &str) -> Result<Option<T>> {
        let entity = self
            .db
            .collection::<T>(self.collection_name)
            .find_one(doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON })
            .await?;
        Ok(entity)
    }

    /// 在调用方事务中根据 ID 查找未删除实体。
    ///
    /// # 参数
    /// * `id` - 实体 ID
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 返回值
    /// 返回事务快照中匹配的未删除实体。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id_with_session(&self, id: &str, session: &mut ClientSession) -> Result<Option<T>> {
        let entity = self
            .db
            .collection::<T>(self.collection_name)
            .find_one(doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON })
            .session(session)
            .await?;
        Ok(entity)
    }

    /// 更新实体（带乐观锁）
    ///
    /// # 参数
    /// * `entity` - 实体对象
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn update(&self, entity: &mut T) -> Result<()>
    where
        T: HasBaseModel,
    {
        if entity.base().is_deleted() {
            return Err(Error::OptimisticLockingError);
        }

        let metadata = write_metadata(entity.base())?;
        let filter = active_cas_filter(entity.base(), metadata);
        let mut document = to_document(&*entity)?;
        document.insert("version", metadata.next_version_bson);
        document.insert("updated_at", metadata.updated_at_bson);

        let result = self
            .db
            .collection::<T>(self.collection_name)
            .update_one(filter, doc! { "$set": document })
            .await?;

        apply_write_result(entity.base_mut(), metadata, None, result.matched_count)
    }

    /// 更新实体（带事务和乐观锁）
    ///
    /// # 参数
    /// * `entity` - 实体对象
    /// * `session` - 事务会话
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn update_with_session(&self, entity: &mut T, session: &mut ClientSession) -> Result<()>
    where
        T: HasBaseModel,
    {
        if entity.base().is_deleted() {
            return Err(Error::OptimisticLockingError);
        }

        let metadata = write_metadata(entity.base())?;
        let filter = active_cas_filter(entity.base(), metadata);
        let mut document = to_document(&*entity)?;
        document.insert("version", metadata.next_version_bson);
        document.insert("updated_at", metadata.updated_at_bson);

        let result = self
            .db
            .collection::<T>(self.collection_name)
            .update_one(filter, doc! { "$set": document })
            .session(session)
            .await?;

        apply_write_result(entity.base_mut(), metadata, None, result.matched_count)
    }

    /// 查找所有未删除的实体
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当内部逻辑或依赖操作失败时返回错误。
    pub async fn list_all(&self) -> Result<Vec<T>> {
        let cursor = self
            .db
            .collection::<T>(self.collection_name)
            .find(doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON })
            .await?;
        cursor_to_vec(cursor).await
    }

    /// 在事务中软删除活跃实体。
    ///
    /// # 参数
    /// * `entity` - 待删除实体
    /// * `session` - 事务会话
    ///
    /// # 错误
    /// 当实体已删除、版本冲突或底层写入失败时返回错误。
    pub async fn soft_delete_with_session(&self, entity: &mut T, session: &mut ClientSession) -> Result<()>
    where
        T: HasBaseModel,
    {
        if entity.base().is_deleted() {
            return Err(Error::OptimisticLockingError);
        }

        let metadata = write_metadata(entity.base())?;
        let filter = active_cas_filter(entity.base(), metadata);
        let result = self
            .db
            .collection::<T>(self.collection_name)
            .update_one(
                filter,
                doc! {
                    "$set": {
                        "version": metadata.next_version_bson,
                        "updated_at": metadata.updated_at_bson,
                        "deleted_at": metadata.updated_at_bson,
                    }
                },
            )
            .session(session)
            .await?;

        apply_write_result(
            entity.base_mut(),
            metadata,
            Some(metadata.updated_at),
            result.matched_count,
        )
    }

    /// 在事务中恢复已软删除实体。
    ///
    /// # 参数
    /// * `entity` - 待恢复实体
    /// * `session` - 事务会话
    ///
    /// # 错误
    /// 当实体未删除、版本冲突或底层写入失败时返回错误。
    pub async fn restore_with_session(&self, entity: &mut T, session: &mut ClientSession) -> Result<()>
    where
        T: HasBaseModel,
    {
        if !entity.base().is_deleted() {
            return Err(Error::OptimisticLockingError);
        }

        let metadata = write_metadata(entity.base())?;
        let filter = deleted_cas_filter(entity.base(), metadata);
        let result = self
            .db
            .collection::<T>(self.collection_name)
            .update_one(
                filter,
                doc! {
                    "$set": {
                        "version": metadata.next_version_bson,
                        "updated_at": metadata.updated_at_bson,
                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                    }
                },
            )
            .session(session)
            .await?;

        apply_write_result(
            entity.base_mut(),
            metadata,
            Some(NOT_DELETED_TIMESTAMP),
            result.matched_count,
        )
    }

    /// 根据单个字段查找一个实体
    ///
    /// # 参数
    /// * `field` - 字段名
    /// * `value` - 值
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn find_one_by_field<V>(&self, field: &str, value: V) -> Result<Option<T>>
    where
        V: Into<mongodb::bson::Bson> + Send,
    {
        let filter = doc! { field: value.into(), "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        let entity = self
            .db
            .collection::<T>(self.collection_name)
            .find_one(filter)
            .await?;
        Ok(entity)
    }

    /// 查找单个实体
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn find_one(&self, filter: Document) -> Result<Option<T>> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);

        let entity = self
            .db
            .collection::<T>(self.collection_name)
            .find_one(filter)
            .await?;
        Ok(entity)
    }

    /// 查找单个实体（带事务）
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `session` - 事务会话
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn find_one_with_session(
        &self,
        filter: Document,
        session: &mut ClientSession,
    ) -> Result<Option<T>> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);

        let entity = self
            .db
            .collection::<T>(self.collection_name)
            .find_one(filter)
            .session(session)
            .await?;
        Ok(entity)
    }

    /// 在调用方事务中判断是否存在符合条件的活跃实体。
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 返回值
    /// 同一事务快照内存在匹配实体时返回 `true`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn exists_with_session(&self, filter: Document, session: &mut ClientSession) -> Result<bool> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
        let document = self
            .db
            .collection::<Document>(self.collection_name)
            .find_one(filter)
            .projection(doc! { "_id": 1 })
            .session(session)
            .await?;
        Ok(document.is_some())
    }

    /// 查找多个实体
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn find_many(&self, filter: Document) -> Result<Vec<T>> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);

        let cursor = self.db.collection::<T>(self.collection_name).find(filter).await?;
        cursor_to_vec(cursor).await
    }

    /// 在调用方事务中查找多个实体。
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 返回
    /// 返回符合条件且未删除的实体集合。
    ///
    /// # 错误
    /// 当查询或游标读取失败时返回错误。
    pub async fn find_many_with_session(
        &self,
        filter: Document,
        session: &mut ClientSession,
    ) -> Result<Vec<T>> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
        let mut cursor = self
            .db
            .collection::<T>(self.collection_name)
            .find(filter)
            .session(&mut *session)
            .await?;
        let mut entities = Vec::new();
        while let Some(entity) = cursor.next(&mut *session).await.transpose()? {
            entities.push(entity);
        }
        Ok(entities)
    }

    /// 查找多个实体（带排序）
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `sort` - 排序条件
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn find_many_sorted(&self, filter: Document, sort: Document) -> Result<Vec<T>> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);

        let cursor = self
            .db
            .collection::<T>(self.collection_name)
            .find(filter)
            .sort(sort)
            .await?;
        cursor_to_vec(cursor).await
    }

    /// 判断是否存在符合条件的活跃实体。
    ///
    /// 查询只投影 MongoDB `_id`，并通过 `find_one` 在首条命中后停止，
    /// 避免为存在性判断加载完整实体或结果集合。
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    ///
    /// # 返回值
    /// 存在匹配实体时返回 `true`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn exists(&self, filter: Document) -> Result<bool> {
        let mut filter = filter;
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);

        let document = self
            .db
            .collection::<Document>(self.collection_name)
            .find_one(filter)
            .projection(doc! { "_id": 1 })
            .await?;
        Ok(document.is_some())
    }

    /// Searches entities with pagination
    ///
    /// # Arguments
    ///
    /// * `filter` - Filter and pagination criteria
    ///
    /// # Returns
    ///
    /// Paginated result containing matched items and total count
    pub async fn search<F>(&self, filter: &F) -> Result<PageResult<T>>
    where
        F: QueryFilter + Pagination + Send + Sync,
    {
        let cursor = self
            .database()
            .collection::<T>(self.collection_name())
            .find(filter.to_doc())
            .skip(filter.skip())
            .limit(filter.limit())
            .sort(doc! { "created_at": -1 })
            .await?;

        let items = cursor_to_vec(cursor).await?;
        let total = self.search_count(filter).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// Counts total number of entities matching a filter
    ///
    /// # Arguments
    ///
    /// * `filter` - Filter criteria
    ///
    /// # Returns
    ///
    /// Total count of matching documents
    async fn search_count<F>(&self, filter: &F) -> Result<u64>
    where
        F: QueryFilter + Send + Sync,
    {
        let count = self
            .database()
            .collection::<T>(self.collection_name())
            .count_documents(filter.to_doc())
            .await?;

        Ok(count)
    }

    /// 获取数据库引用（内部使用）
    ///
    /// # 返回
    /// 返回引用，生命周期与持有者一致。
    pub(crate) fn database(&self) -> &Database {
        self.db
    }

    /// 获取集合名称（内部使用）
    ///
    /// # 返回
    /// 返回引用，生命周期与持有者一致。
    pub(crate) fn collection_name(&self) -> &str {
        self.collection_name
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
