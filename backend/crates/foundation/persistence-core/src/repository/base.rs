//! 通用 Repository 实现。
//!
//! 提供 MongoDB 数据库操作的通用接口，包括基础 CRUD。

use entity_core::{BaseModel, HasBaseModel, NOT_DELETED_TIMESTAMP, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::bson::{Document, deserialize_from_slice, doc, serialize_to_vec};
use mongodb::options::FindOptions;
use mongodb::{Collection, Database};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::change_feed::ChangeSource;
use crate::errors::{Error, Result};
use crate::{Executor, mongo_ops};

/// 定义数据库查询的过滤条件。
///
/// 由提供查询过滤条件的类型实现。
pub trait QueryFilter {
    /// 由领域筛选器生成 MongoDB 条件；通用分页查询按原样同时用于取数和计数。
    fn to_doc(&self) -> Document;
}

/// 定义数据库查询的分页参数。
///
/// 由提供分页参数的类型实现。
pub trait Pagination {
    /// 提供计算分页偏移与上限的 `(页码, 每页条数)`。页码从一开始；旧调用返回零时，默认偏移仍从第一页开始。
    fn page_and_size(&self) -> (u64, u64);

    /// 给查询提供 `(page.max(1) - 1) * page_size` 的偏移，将页码零视为首页。
    fn skip(&self) -> u64 {
        let (page, page_size) = self.page_and_size();
        (page.max(1) - 1) * page_size
    }

    /// 为 MongoDB 查询提供单页上限；每页条数超过 `i64` 时取 `i64::MAX`，不截断成更小或负的值。
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

// 分页条数或总数必须用 `i64` 表达时，超限取上界，防止截断改变查询和响应语义。
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

// 构造乐观锁写入前捕获一次当前秒，用于计算待比较版本、新版本和更新时间。
fn write_metadata(base: &BaseModel) -> Result<WriteMetadata> {
    write_metadata_at(base, chrono::Local::now().timestamp())
}

// 从实体版本和指定时间构造写入元数据，同时校验版本加一及 BSON 表示范围；失败时不改内存实体。
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

// 更新、删除和恢复共用业务 ID 与期望版本条件，再按操作限定已删除或活跃状态，以免并发写错状态。
fn cas_filter(base: &BaseModel, metadata: WriteMetadata, expected_deleted: bool) -> Document {
    let mut filter = doc! {
        "id": &base.id,
        "version": metadata.expected_version,
    };
    if expected_deleted {
        filter.insert("deleted_at", doc! { "$ne": NOT_DELETED_TIMESTAMP_BSON });
    } else {
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    }
    filter
}

// 统一给日常读取加未删除条件，覆盖同名过滤值，防止仓储漏加软删约束。
fn with_active_scope(mut filter: Document) -> Document {
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    filter
}

// CAS 命中才将新版本、更新时间及可选删除时间同步到实体；零命中返回乐观锁冲突且不改内存。
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

// 组装 CAS 前先核对内存实体的删除状态，再生成版本元数据和过滤；状态不合预期时不写数据库。
fn cas_metadata_and_filter(base: &BaseModel, expected_deleted: bool) -> Result<(WriteMetadata, Document)> {
    if base.is_deleted() != expected_deleted {
        return Err(Error::OptimisticLockingError);
    }
    let metadata = write_metadata(base)?;
    Ok((metadata, cas_filter(base, metadata, expected_deleted)))
}

// 用原执行器做不带 upsert 的比较写入，只有匹配时才同步内存元数据，避免把过期实体当作已保存。
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
    /// 领域仓储初始化时绑定数据库和本域集合，使后续通用操作按实体类型取集合；构造本身不执行 I/O。
    pub fn new(db: &'a Database, collection_name: &'a str) -> Self {
        Self { db, collection_name, _phantom: std::marker::PhantomData }
    }

    /// 为本仓储的集合登记已提交变更的共享刷新主题，供统一订阅器使用；不读取或暴露业务文档。
    pub fn changes<U>(&self, topic: U) -> ChangeSource<U> {
        ChangeSource::shared(self.collection_name, topic)
    }

    /// 需要同一数据库上的领域专用操作时，借用仓储已绑定的句柄；事务仍由调用方执行器决定。
    pub fn database(&self) -> &'a Database {
        self.db
    }

    /// 新建领域实体时沿用 Service 提供的执行器插入集合。重复唯一键和其他数据库写入失败保留仓储分类。
    pub async fn create(&self, entity: &T, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), entity, executor).await
    }

    /// 普通详情读取按业务 ID 取未删除实体；未命中返回 `None`，沿用本次读取执行器。
    pub async fn find_by_id(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<T>> {
        self.find_one(doc! { "id": id }, executor).await
    }

    /// 恢复或追溯场景可以按业务 ID 读取历史软删除记录；无匹配仍返回 `None`，不用于默认活跃列表。
    pub async fn find_by_id_including_deleted(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<T>> {
        mongo_ops::find_one(&self.collection(), doc! { "id": id }, executor).await
    }

    /// 需要按唯一业务键等字段追溯记录时，精确取首个匹配值，不排除软删除；无匹配返回 `None`。
    pub async fn find_one_by_field_including_deleted<V>(
        &self,
        field: &str,
        value: V,
        executor: &mut dyn Executor,
    ) -> Result<Option<T>>
    where
        V: Into<mongodb::bson::Bson> + Send,
    {
        mongo_ops::find_one(&self.collection(), doc! { field: value.into() }, executor).await
    }

    /// 按外键追溯全量关联记录时，取该字段的所有精确匹配，不过滤软删除，也不附加排序或分页。
    pub async fn find_many_by_field_including_deleted<V>(
        &self,
        field: &str,
        value: V,
        executor: &mut dyn Executor,
    ) -> Result<Vec<T>>
    where
        V: Into<mongodb::bson::Bson> + Send,
    {
        mongo_ops::find_many(
            &self.collection(),
            doc! { field: value.into() },
            FindOptions::builder().build(),
            executor,
        )
        .await
    }

    /// 保存活跃实体时按原版本比较写入；状态或版本不匹配返回乐观锁错误，命中后才同步内存版本和更新时间。
    /// 方法一开始就撤销旧读取凭证，即使前置校验或 BSON 编码失败也不会保留它。
    /// 版本或时间越界返回 `EntityMetadataOutOfRange`，BSON 编码错误单独返回。
    pub async fn update(&self, entity: &mut T, executor: &mut dyn Executor) -> Result<()>
    where
        T: HasBaseModel,
    {
        executor.next_read();
        let (metadata, filter) = cas_metadata_and_filter(entity.base(), false)?;

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

    /// 删除业务对象但需保留历史时，按原版本匹配活跃实体；命中后将删除时间设为本次更新时间，并更新内存版本和时间。
    /// 旧读取凭证在前置校验前即失效。已删除或版本未命中报乐观锁错误，元数据越界则单独报错。
    pub async fn soft_delete(&self, entity: &mut T, executor: &mut dyn Executor) -> Result<()>
    where
        T: HasBaseModel,
    {
        self.set_deleted(entity, true, executor).await
    }

    /// 恢复历史软删除实体时比较原版本，命中后清除删除标记并同步内存版本和时间。开始即使旧读取凭证失效；
    /// 原记录未删除、版本冲突或元数据越界时拒绝写入。
    pub async fn restore(&self, entity: &mut T, executor: &mut dyn Executor) -> Result<()>
    where
        T: HasBaseModel,
    {
        self.set_deleted(entity, false, executor).await
    }

    async fn set_deleted(&self, entity: &mut T, deleted: bool, executor: &mut dyn Executor) -> Result<()>
    where
        T: HasBaseModel,
    {
        executor.next_read();
        let (metadata, filter) = cas_metadata_and_filter(entity.base(), !deleted)?;

        let (deleted_at, deleted_at_bson) = if deleted {
            (metadata.updated_at, metadata.updated_at_bson)
        } else {
            (NOT_DELETED_TIMESTAMP, NOT_DELETED_TIMESTAMP_BSON)
        };
        let update = doc! {
            "$set": {
                "version": metadata.next_version_bson,
                "updated_at": metadata.updated_at_bson,
                "deleted_at": deleted_at_bson,
            }
        };

        exec_cas(&self.collection(), entity.base_mut(), metadata, filter, update, Some(deleted_at), executor)
            .await
    }

    /// 无需额外筛选且调用方能承受全量结果时，沿用当前执行器读全部未删除实体，不附加排序或分页。
    pub async fn list_all(&self, executor: &mut dyn Executor) -> Result<Vec<T>> {
        self.find_many_scoped(Document::new(), FindOptions::default(), executor).await
    }

    /// 按单个业务字段精确定位活跃实体；会排除软删除记录，未匹配时返回 `None`。
    pub async fn find_one_by_field<V>(
        &self,
        field: &str,
        value: V,
        executor: &mut dyn Executor,
    ) -> Result<Option<T>>
    where
        V: Into<mongodb::bson::Bson> + Send,
    {
        self.find_one(doc! { field: value.into() }, executor).await
    }

    /// 领域已组装完整过滤条件时，附加未删除约束并取首条匹配实体。无匹配返回 `None`，读失败向上返回。
    pub async fn find_one(&self, filter: Document, executor: &mut dyn Executor) -> Result<Option<T>> {
        mongo_ops::find_one(&self.collection(), with_active_scope(filter), executor).await
    }

    /// 批量取符合领域条件的活跃实体，不提供固定顺序；需要排序时用 [`Self::find_many_sorted`]。
    pub async fn find_many(&self, filter: Document, executor: &mut dyn Executor) -> Result<Vec<T>> {
        self.find_many_scoped(filter, FindOptions::default(), executor).await
    }

    /// 当列表或处理顺序明确时，按调用方的排序文档读取活跃匹配项，不隐式分页。
    pub async fn find_many_sorted(
        &self,
        filter: Document,
        sort: Document,
        executor: &mut dyn Executor,
    ) -> Result<Vec<T>> {
        self.find_many_scoped(filter, FindOptions::builder().sort(sort).build(), executor).await
    }

    /// 解析一组稳定业务引用时批量取未删除对象，不按输入顺序补位；空 ID 集合直接返回，不发起查询。
    pub async fn list_active_by_ids(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<T>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 唯一性检查或前置条件只关心有无活跃记录时，附加软删约束并用 `_id` 投影探测，不读取完整实体。
    pub async fn exists(&self, filter: Document, executor: &mut dyn Executor) -> Result<bool> {
        mongo_ops::exists(&self.collection(), with_active_scope(filter), executor).await
    }

    /// 用同一领域条件在原执行器中取页和计数，固定按 `created_at` 降序。不会自动排除软删除，过滤由调用方指定；
    /// 总数超出 `i64` 时取上界。
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

    /// 领域仓储需要实现通用入口未覆盖的操作时，从既有仓储取绑定实体的 MongoDB 集合句柄。读写继续使用调用方执行器。
    pub fn collection(&self) -> Collection<T> {
        self.db.collection::<T>(self.collection_name)
    }

    // 集中为列表查询补上活跃约束，保留各入口给出的排序、分页和执行器。
    async fn find_many_scoped(
        &self,
        filter: Document,
        options: FindOptions,
        executor: &mut dyn Executor,
    ) -> Result<Vec<T>> {
        mongo_ops::find_many(&self.collection(), with_active_scope(filter), options, executor).await
    }
}

/// 把实体快照编码为与 MongoDB 插入相同的非 human-readable BSON，再取出文档用于 `$set`，
/// 使 Decimal128 等原生类型在更新时不变成字符串；编码或文档解码失败时返回错误。
pub fn persisted_document<T: Serialize>(value: &T) -> Result<Document> {
    Ok(deserialize_from_slice(&serialize_to_vec(value)?)?)
}

#[cfg(test)]
mod tests {
    use entity_core::{BaseModel, HasBaseModel, NOT_DELETED_TIMESTAMP_BSON};
    use mongodb::Client;
    use mongodb::bson::doc;
    use mongodb::options::{ClientOptions, ServerAddress};
    use serde::ser::Error as _;
    use serde::{Deserialize, Serialize, Serializer};

    use super::{
        Pagination, Repository, apply_write_result, cas_filter, persisted_document, write_metadata_at,
    };
    use crate::errors::Error;
    use crate::{Executor, ReadExecutor};

    #[derive(Deserialize)]
    struct TestEntity {
        #[serde(flatten)]
        base: BaseModel,
    }

    impl Serialize for TestEntity {
        fn serialize<S: Serializer>(&self, _: S) -> std::result::Result<S::Ok, S::Error> {
            Err(S::Error::custom("entity serialization rejected"))
        }
    }

    impl HasBaseModel for TestEntity {
        fn base(&self) -> &BaseModel {
            &self.base
        }

        fn base_mut(&mut self) -> &mut BaseModel {
            &mut self.base
        }
    }

    #[derive(Clone, Copy)]
    enum Write {
        Update,
        SoftDelete,
        Restore,
    }

    #[tokio::test]
    async fn failed_repository_preconditions_invalidate_old_reads_without_mutating_entities() {
        let options = ClientOptions::builder()
            .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
            .build();
        let db = Client::with_options(options).unwrap().database("unit_test");
        let repository = Repository::<TestEntity>::new(&db, "documents");
        let cases = [
            (Write::Update, 1, 1),
            (Write::SoftDelete, 1, 1),
            (Write::Restore, 0, 1),
            (Write::Restore, 0, u64::MAX),
            (Write::Update, 0, u64::MAX),
            (Write::SoftDelete, 0, u64::MAX),
            (Write::Restore, 1, u64::MAX),
            (Write::Update, 0, 1),
        ];

        for (write, deleted_at, version) in cases {
            let base = BaseModel { deleted_at, version, ..BaseModel::fake() };
            let mut entity = TestEntity { base: base.clone() };
            let mut executor = ReadExecutor::default();
            let before = executor.boundary().unwrap();

            let result = match write {
                Write::Update => repository.update(&mut entity, &mut executor).await,
                Write::SoftDelete => repository.soft_delete(&mut entity, &mut executor).await,
                Write::Restore => repository.restore(&mut entity, &mut executor).await,
            };
            let error = result.unwrap_err();
            if (deleted_at != 0) != matches!(write, Write::Restore) {
                assert!(matches!(error, Error::OptimisticLockingError));
            } else if version == u64::MAX {
                assert!(matches!(error, Error::EntityMetadataOutOfRange("version")));
            } else {
                assert!(matches!(error, Error::BsonError(_)));
            }

            assert!(!before.is_current(&executor));
            assert!(executor.boundary().unwrap().is_current(&executor));
            assert_eq!(entity.base, base);
        }
    }

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

        let active = cas_filter(&base, metadata, false);
        assert_eq!(active.get_str("id").expect("id should be present"), "fake");
        assert_eq!(active.get_i64("version").expect("version should be present"), 1);
        assert_eq!(
            active.get_i64("deleted_at").expect("deleted_at should be present"),
            NOT_DELETED_TIMESTAMP_BSON
        );

        let deleted = cas_filter(&base, metadata, true);
        assert_eq!(
            deleted.get_document("deleted_at").expect("deleted scope should be present"),
            &doc! { "$ne": NOT_DELETED_TIMESTAMP_BSON }
        );
    }
}
