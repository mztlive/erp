//! 通用Repository实现
//!
//! 提供MongoDB数据库操作的通用接口，包括基础CRUD操作和各实体的特化方法

use entity_core::{BaseModel, HasBaseModel, NOT_DELETED_TIMESTAMP, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::bson::{Document, deserialize_from_slice, doc, serialize_to_vec};
use mongodb::options::FindOptions;
use mongodb::{Collection, Database};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::errors::{Error, Result};
use crate::{Executor, mongo_ops};

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
        saturating_i64(page_size)
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

/// 将 `u64` 饱和转换为 `i64`，极端值钳制到 `i64::MAX` 而非静默截断。
fn saturating_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
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
    let next_version = base.version.checked_add(1).ok_or(Error::EntityMetadataOutOfRange("version"))?;
    let expected_version =
        i64::try_from(base.version).map_err(|_| Error::EntityMetadataOutOfRange("version"))?;
    let next_version_bson =
        i64::try_from(next_version).map_err(|_| Error::EntityMetadataOutOfRange("version"))?;
    let updated_at =
        u64::try_from(updated_at_bson).map_err(|_| Error::EntityMetadataOutOfRange("updated_at"))?;

    Ok(WriteMetadata { expected_version, next_version, next_version_bson, updated_at, updated_at_bson })
}

/// 构建 CAS 写入的乐观锁过滤条件。
///
/// `active_scope` 为 `true` 时限定活跃实体，为 `false` 时限定已删除实体；
/// 两分支仅 `deleted_at` 子句不同，组装收敛一处避免过滤条件漂移。
fn cas_filter(base: &BaseModel, metadata: WriteMetadata, active_scope: bool) -> Document {
    let mut filter = doc! {
        "id": &base.id,
        "version": metadata.expected_version,
    };
    if active_scope {
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    } else {
        filter.insert("deleted_at", doc! { "$ne": NOT_DELETED_TIMESTAMP_BSON });
    }
    filter
}

/// 拒绝已删除实体的 CAS 写入（更新与软删除仅接受活跃实体）。
fn ensure_active_for_cas(base: &BaseModel) -> Result<()> {
    if base.is_deleted() {
        return Err(Error::OptimisticLockingError);
    }
    Ok(())
}

/// 拒绝活跃实体的恢复（恢复仅接受已删除实体）。
fn ensure_deleted_for_restore(base: &BaseModel) -> Result<()> {
    if !base.is_deleted() {
        return Err(Error::OptimisticLockingError);
    }
    Ok(())
}

/// 为查询附加软删除活跃约束，新增查询统一经此入口，避免遗漏 `deleted_at` 域。
fn with_active_scope(mut filter: Document) -> Document {
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    filter
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

/// 组装一次 CAS 写入所需的元数据与乐观锁过滤器。
fn cas_metadata_and_filter(base: &BaseModel, active_scope: bool) -> Result<(WriteMetadata, Document)> {
    let metadata = write_metadata(base)?;
    Ok((metadata, cas_filter(base, metadata, active_scope)))
}

/// 执行 CAS 更新并同步内存元数据。
async fn exec_cas<T>(
    collection: &Collection<T>,
    base: &mut BaseModel,
    metadata: WriteMetadata,
    filter: Document,
    update: Document,
    deleted_at: Option<u64>,
    executor: &mut dyn Executor,
) -> Result<()>
where
    T: Send + Sync,
{
    let result = mongo_ops::update_one(collection, filter, update, false, executor).await?;
    apply_write_result(base, metadata, deleted_at, result.matched_count)
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
        Self { db, collection_name, _phantom: std::marker::PhantomData }
    }

    /// Returns the MongoDB database handle bound to this repository.
    ///
    /// # Returns
    /// The database handle used to construct this repository.
    pub fn database(&self) -> &'a Database {
        self.db
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
        self.find_one_scoped(doc! { "id": id }, executor).await
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
        ensure_active_for_cas(entity.base())?;

        let (metadata, filter) = cas_metadata_and_filter(entity.base(), true)?;
        let mut document = persisted_document(&*entity)?;
        document.insert("version", metadata.next_version_bson);
        document.insert("updated_at", metadata.updated_at_bson);

        exec_cas(
            &self.collection(),
            entity.base_mut(),
            metadata,
            filter,
            doc! { "$set": document },
            None,
            executor,
        )
        .await
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
        ensure_active_for_cas(entity.base())?;

        let (metadata, filter) = cas_metadata_and_filter(entity.base(), true)?;
        let deleted_at = metadata.updated_at;
        let update = doc! {
            "$set": {
                "version": metadata.next_version_bson,
                "updated_at": metadata.updated_at_bson,
                "deleted_at": metadata.updated_at_bson,
            }
        };

        exec_cas(&self.collection(), entity.base_mut(), metadata, filter, update, Some(deleted_at), executor)
            .await
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
        ensure_deleted_for_restore(entity.base())?;

        let (metadata, filter) = cas_metadata_and_filter(entity.base(), false)?;
        let update = doc! {
            "$set": {
                "version": metadata.next_version_bson,
                "updated_at": metadata.updated_at_bson,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        };

        exec_cas(
            &self.collection(),
            entity.base_mut(),
            metadata,
            filter,
            update,
            Some(NOT_DELETED_TIMESTAMP),
            executor,
        )
        .await
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
        self.find_many_scoped(Document::new(), FindOptions::default(), executor).await
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
    pub async fn find_one_by_field<V>(
        &self,
        field: &str,
        value: V,
        executor: &mut dyn Executor,
    ) -> Result<Option<T>>
    where
        V: Into<mongodb::bson::Bson> + Send,
    {
        let filter = doc! { field: value.into() };
        self.find_one_scoped(filter, executor).await
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
    pub async fn find_one(&self, filter: Document, executor: &mut dyn Executor) -> Result<Option<T>> {
        self.find_one_scoped(filter, executor).await
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
    pub async fn find_many(&self, filter: Document, executor: &mut dyn Executor) -> Result<Vec<T>> {
        self.find_many_scoped(filter, FindOptions::default(), executor).await
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
    pub async fn find_many_sorted(
        &self,
        filter: Document,
        sort: Document,
        executor: &mut dyn Executor,
    ) -> Result<Vec<T>> {
        self.find_many_scoped(filter, FindOptions::builder().sort(sort).build(), executor).await
    }

    /// 按稳定 ID 批量读取未删除实体。
    ///
    /// 通用按 ID 批量加载原语，不绑定具体业务类型。
    ///
    /// # 参数
    /// * `ids` - 业务对象稳定 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的业务对象；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_active_by_ids(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<T>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
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
    pub async fn exists(&self, filter: Document, executor: &mut dyn Executor) -> Result<bool> {
        mongo_ops::exists(&self.collection(), with_active_scope(filter), executor).await
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
    pub async fn search<F>(&self, filter: &F, executor: &mut dyn Executor) -> Result<PageResult<T>>
    where
        F: QueryFilter + Pagination + Send + Sync,
    {
        let filter_doc = filter.to_doc();
        let items = mongo_ops::find_many(
            &self.collection(),
            filter_doc.clone(),
            FindOptions::builder()
                .sort(doc! { "created_at": -1 })
                .skip(filter.skip())
                .limit(filter.limit())
                .build(),
            &mut *executor,
        )
        .await?;
        let total = mongo_ops::count_documents(&self.collection(), filter_doc, executor).await?;

        Ok(PageResult { items, total: saturating_i64(total) })
    }

    /// 获取当前实体对应的 MongoDB 集合（内部使用）。
    ///
    /// # 返回
    /// 返回按实体类型参数化的集合句柄。
    pub fn collection(&self) -> Collection<T> {
        self.db.collection::<T>(self.collection_name)
    }

    /// 以活跃域约束查询单个文档（各单条读取的共用入口）。
    ///
    /// # 参数
    /// * `filter` - 未加域的过滤条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除实体；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_one_scoped(&self, filter: Document, executor: &mut dyn Executor) -> Result<Option<T>> {
        mongo_ops::find_one(&self.collection(), with_active_scope(filter), executor).await
    }

    /// 以活跃域约束查询多个文档（各列表读取的共用入口）。
    ///
    /// # 参数
    /// * `filter` - 未加域的过滤条件
    /// * `options` - 排序与分页等查询选项
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回符合条件且未删除的实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_many_scoped(
        &self,
        filter: Document,
        options: FindOptions,
        executor: &mut dyn Executor,
    ) -> Result<Vec<T>> {
        mongo_ops::find_many(&self.collection(), with_active_scope(filter), options, executor).await
    }
}

/// 更新与 MongoDB 插入使用相同的非 human-readable BSON 形态，保持数值原生类型。
///
/// # 参数
/// * `value` - 待写入的实体快照
/// # 返回
/// 返回可用于 `$set` 的原生 BSON 文档。
/// # 错误
/// 实体无法编码为 BSON 或解码为文档时返回序列化错误。
fn persisted_document<T: Serialize>(value: &T) -> Result<Document> {
    Ok(deserialize_from_slice(&serialize_to_vec(value)?)?)
}

#[cfg(test)]
mod tests {
    use entity_core::{BaseModel, NOT_DELETED_TIMESTAMP_BSON};
    use mongodb::bson::doc;

    use super::{
        Pagination, apply_write_result, cas_filter, ensure_active_for_cas, ensure_deleted_for_restore,
        persisted_document, write_metadata_at,
    };
    use crate::errors::Error;

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
    fn update_document_preserves_optional_decimal_wire_type() {
        use std::str::FromStr;

        use mongodb::bson::{Bson, Decimal128};
        use serde::{Serialize, Serializer};

        struct Quantity;
        impl Serialize for Quantity {
            fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
                if serializer.is_human_readable() {
                    serializer.serialize_str("260")
                } else {
                    Decimal128::from_str("260").unwrap().serialize(serializer)
                }
            }
        }
        #[derive(Serialize)]
        struct Availability {
            available_quantity: Option<Quantity>,
        }
        let document = persisted_document(&Availability { available_quantity: Some(Quantity) }).unwrap();
        assert!(matches!(document.get("available_quantity"), Some(Bson::Decimal128(_))));
        let absent = persisted_document(&Availability { available_quantity: None }).unwrap();
        assert_eq!(absent.get("available_quantity"), Some(&Bson::Null));
    }

    #[test]
    fn pagination_normalizes_zero_to_first_page() {
        let pagination = TestPagination { page: 0, page_size: 20 };

        assert_eq!(pagination.skip(), 0);
        assert_eq!(pagination.limit(), 20);
    }

    #[test]
    fn pagination_calculates_offset_for_regular_page() {
        let pagination = TestPagination { page: 3, page_size: 20 };

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

    #[test]
    fn cas_filter_branches_share_identity_and_differ_only_in_deleted_scope() {
        let base = BaseModel::fake();
        let metadata = write_metadata_at(&base, 1_700_000_000).expect("metadata should be valid");

        let active = cas_filter(&base, metadata, true);
        assert_eq!(active.get_str("id").expect("id should be present"), "fake");
        assert_eq!(active.get_i64("version").expect("version should be present"), 1);
        assert_eq!(
            active.get_i64("deleted_at").expect("deleted_at should be present"),
            NOT_DELETED_TIMESTAMP_BSON
        );

        let deleted = cas_filter(&base, metadata, false);
        assert_eq!(
            deleted.get_document("deleted_at").expect("deleted scope should be present"),
            &doc! { "$ne": NOT_DELETED_TIMESTAMP_BSON }
        );
    }

    #[test]
    fn cas_guards_accept_only_matching_lifecycle_state() {
        let active = BaseModel::fake();

        ensure_active_for_cas(&active).expect("active entity should be writable");
        assert!(matches!(ensure_deleted_for_restore(&active), Err(Error::OptimisticLockingError)));

        let deleted = BaseModel { deleted_at: 1_700_000_001, ..BaseModel::fake() };

        ensure_deleted_for_restore(&deleted).expect("deleted entity should be restorable");
        assert!(matches!(ensure_active_for_cas(&deleted), Err(Error::OptimisticLockingError)));
    }
}
