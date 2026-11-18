//! 域 D11 `warehouse` 仓储：warehouse、warehouse_revision、warehouse_sku_policy。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充域特有
//! 查询与跨集合多步骤写入入口。集合名常量统一从 `WarehouseExt` 关联常量导入。
//!
//! `warehouse_revision` 与 `warehouse_sku_policy` 的敏感字段（地址/联系人加密列）
//! 不进入任何列表投影；`warehouse_revision` 是追加写入修订，不提供软删除方法。
//!
//! 筛选/行类型定义在本文件，经 `repository/mod.rs` 再导出；`WarehouseExt` 仍保留
//! 同名关联类型，不删除。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SkuId, WarehouseId};
use erp_core::money::Quantity;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::extensions::WarehouseExt;
use super::warehouse_projection::{
    warehouse_projection, warehouse_revision_projection, warehouse_sku_policy_projection,
};
// 排序白名单唯一来源：DTO 校验侧同名常量；仓储排序回落与之同源，禁止漂移。
use crate::dto::warehouse::{
    WAREHOUSE_REVISION_SORT_FIELDS, WAREHOUSE_SKU_POLICY_SORT_FIELDS, WAREHOUSE_SORT_FIELDS,
};
use crate::entity::warehouse::{EnableStatus, Warehouse, WarehouseRevision, WarehouseSkuPolicy};

/// `warehouse` 集合名（单一来源：`WarehouseExt` 关联常量）。
const WAREHOUSES: &str = <mongodb::Database as WarehouseExt>::WAREHOUSES;
/// `warehouse_revision` 集合名（单一来源：`WarehouseExt` 关联常量）。
const WAREHOUSE_REVISIONS: &str = <mongodb::Database as WarehouseExt>::WAREHOUSE_REVISIONS;
/// `warehouse_sku_policy` 集合名。
const WAREHOUSE_SKU_POLICIES: &str = <mongodb::Database as WarehouseExt>::WAREHOUSE_SKU_POLICIES;

/// 三类列表筛选的 `page`/`page_size` 分页 trait。
macro_rules! filter_pagination {
    ($filter:ty) => {
        impl Pagination for $filter {
            // 让三类仓库列表统一交出原页码和扩为 `u64` 的条数，不在仓储把小于 `1` 的页码修正为首页。
            fn page_and_size(&self) -> (u64, u64) {
                (self.page, u64::from(self.page_size))
            }
        }
    };
}

/// 三类列表筛选的首页空筛选默认值（`page: 1`，`page_size: 20`，降序）。
macro_rules! filter_default {
    ($ty:ident { $($field:ident: $init:expr),* $(,)? }) => {
        impl Default for $ty {
            // 为未指定筛选的三类列表提供首页、每页 `20` 条和降序设置。
            fn default() -> Self {
                Self { $($field: $init,)* page: 1, page_size: 20, sort_by: None, sort_ascending: false }
            }
        }
    };
}

/// 仓库列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseRow {
    /// 实体主键。
    pub id: String,
    /// ERP 仓库稳定代码。
    pub warehouse_code: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 入库经办人。
    #[serde(default)]
    pub inbound_handler_user_id: Option<String>,
    /// 仓发经办人。
    #[serde(default)]
    pub outbound_handler_user_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 仓库列表筛选条件。
#[derive(Debug, Clone)]
pub struct WarehouseFilter {
    /// 指定仓库身份，与关键词取交集。
    pub warehouse_id: Option<String>,
    /// 必须具有非空入库经办人。
    pub require_inbound_handler: bool,
    /// 代码或当前修订名称关键词，在分页前执行。
    pub q: Option<String>,
    /// 仓库代码精确匹配；`None` 表示不筛选。
    pub warehouse_code: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`warehouse_code`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

filter_default!(WarehouseFilter {
    warehouse_id: None,
    require_inbound_handler: false,
    q: None,
    warehouse_code: None,
    status: None,
});

impl QueryFilter for WarehouseFilter {
    // 生成未删除仓库的 MongoDB 条件：仓库代码、ID 精确匹配，状态按稳定代码筛选。要求有入库经办人时只匹配含非空白字符的字符串；关键词 `q` 和排序由后续阶段处理。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(code) = &self.warehouse_code {
            filter.insert("warehouse_code", code);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(id) = &self.warehouse_id {
            filter.insert("id", id);
        }
        if self.require_inbound_handler {
            filter.insert("inbound_handler_user_id", doc! { "$type": "string", "$regex": r"\S" });
        }
        filter
    }
}

filter_pagination!(WarehouseFilter);

/// 仓库集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait WarehouseRepositoryExt {
    /// 用同一执行器获取仓库列表投影和同条件总数，不读取整文档；只允许 `created_at` 或 `warehouse_code` 排序。MongoDB 查询、游标或计数失败时返回错误。
    async fn search_warehouses(
        &self,
        filter: &WarehouseFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WarehouseRow>>;
}

impl WarehouseRepositoryExt for Repository<'_, Warehouse> {
    async fn search_warehouses(
        &self,
        filter: &WarehouseFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WarehouseRow>> {
        let query = keyword_filter(self, filter, executor).await?;
        search_projected_page(
            &self.collection(),
            query,
            sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, WAREHOUSE_SORT_FIELDS),
            filter.skip(),
            filter.limit(),
            warehouse_projection(),
            executor,
        )
        .await
    }
}

/// 仓库修订列表投影行（不含加密地址/联系人等敏感字段）。
///
/// `SensitiveText` 密文与指纹均不进入列表投影（数据模型 §4.5.5：列表只暴露
/// 名称、有效期与变更原因；详情按权限单独取整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属仓库。
    pub warehouse_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 仓库名称（结构化快照）。
    pub name: String,
    /// 所在地区代码；本字段上线前建立的修订为空。
    #[serde(default)]
    pub region_code: Option<erp_core::region::RegionCode>,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 仓库修订列表筛选条件（修订表追加写入，无软删除过滤）。
#[derive(Debug, Clone)]
pub struct WarehouseRevisionFilter {
    /// 所属仓库；`None` 表示不筛选。
    pub warehouse_id: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

filter_default!(WarehouseRevisionFilter { warehouse_id: None, name: None });

impl QueryFilter for WarehouseRevisionFilter {
    // 构造修订筛选，沿用既有记录的未删除条件，但不提供修订软删除。仓库 ID 精确匹配，名称按字面量忽略大小写，未提供的字段不进入谓词。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(warehouse_id) = &self.warehouse_id {
            filter.insert("warehouse_id", warehouse_id);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        filter
    }
}

filter_pagination!(WarehouseRevisionFilter);

/// 仓库修订集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait WarehouseRevisionRepositoryExt {
    /// 分页读取仓库修订列表及总数，仅返回所需字段，不暴露地址或联系人加密列；排序限定 `created_at`、`revision_no`。查询、游标或计数失败时返回错误。
    async fn search_warehouse_revisions(
        &self,
        filter: &WarehouseRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WarehouseRevisionRow>>;
}

impl WarehouseRevisionRepositoryExt for Repository<'_, WarehouseRevision> {
    async fn search_warehouse_revisions(
        &self,
        filter: &WarehouseRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WarehouseRevisionRow>> {
        search_projected_page(
            &self.collection(),
            filter.to_doc(),
            sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, WAREHOUSE_REVISION_SORT_FIELDS),
            filter.skip(),
            filter.limit(),
            warehouse_revision_projection(),
            executor,
        )
        .await
    }
}

/// 仓库-SKU 预警策略列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseSkuPolicyRow {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: String,
    /// SKU。
    pub sku_id: String,
    /// 最低可用量预警阈值（Decimal128 定点数量）。
    pub minimum_available_quantity: Quantity,
    /// 启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 仓库-SKU 预警策略列表筛选条件。
#[derive(Debug, Clone)]
pub struct WarehouseSkuPolicyFilter {
    /// 仓库；`None` 表示不筛选。
    pub warehouse_id: Option<String>,
    /// SKU；`None` 表示不筛选。
    pub sku_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`effective_from`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for WarehouseSkuPolicyFilter {
    // 构造未删除预警策略的查询条件，按已提供的仓库、SKU 和状态稳定代码精确匹配，省略未设置的条件。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(warehouse_id) = &self.warehouse_id {
            filter.insert("warehouse_id", warehouse_id);
        }
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

filter_pagination!(WarehouseSkuPolicyFilter);

filter_default!(WarehouseSkuPolicyFilter { warehouse_id: None, sku_id: None, status: None });

/// 仓库-SKU 预警策略集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait WarehouseSkuPolicyRepositoryExt {
    /// 分页读取仓库 SKU 策略投影和总数，Decimal128 阈值不舍入或换算，排序限于 `created_at`、`effective_from`。查询、游标或计数失败直接返回。
    async fn search_warehouse_sku_policies(
        &self,
        filter: &WarehouseSkuPolicyFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WarehouseSkuPolicyRow>>;

    /// 用一次 `$in` 查询取回多个 SKU 的未删除仓库策略，避免逐 SKU 加载；查询或游标读取失败返回对应错误。
    async fn find_by_sku_ids(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WarehouseSkuPolicy>>;
}

impl WarehouseSkuPolicyRepositoryExt for Repository<'_, WarehouseSkuPolicy> {
    async fn search_warehouse_sku_policies(
        &self,
        filter: &WarehouseSkuPolicyFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WarehouseSkuPolicyRow>> {
        search_projected_page(
            &self.collection(),
            filter.to_doc(),
            sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, WAREHOUSE_SKU_POLICY_SORT_FIELDS),
            filter.skip(),
            filter.limit(),
            warehouse_sku_policy_projection(),
            executor,
        )
        .await
    }

    async fn find_by_sku_ids(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WarehouseSkuPolicy>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = sku_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "sku_id": { "$in": ids } }, executor).await
    }
}

/// D11 域专用仓储：语义查询与跨集合聚合写入。
///
/// 本类型屏蔽仓库域及其跨域引用的 MongoDB 查询细节，并提供必须由 Service
/// 传入事务执行器的聚合写入入口，由 `WarehouseExt::warehouse()` 访问。
pub struct WarehouseDomainRepository<'a> {
    db: &'a Database,
}

impl<'a> WarehouseDomainRepository<'a> {
    /// 将领域仓储绑定到目标数据库，供仓库与修订等多集合访问共用。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 按实际主键读取未删除仓库供用例核对，不存在返回 `None`，MongoDB 查询失败保持原错误。
    pub async fn warehouse(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<Warehouse>> {
        self.db.warehouses().find_by_id(id, executor).await
    }

    /// 读取当前仓库最大修订序号并加一，供追加修订；无历史时从 `1` 开始，查询或游标失败直接返回。
    pub async fn next_revision_no(&self, warehouse_id: &str, executor: &mut dyn Executor) -> Result<u32> {
        let revisions = mongo_ops::find_many(
            &self.db.collection::<WarehouseRevision>(WAREHOUSE_REVISIONS),
            doc! {
                "warehouse_id": warehouse_id,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().sort(doc! { "revision_no": -1 }).limit(1).build(),
            executor,
        )
        .await?;
        Ok(revisions.first().map(|revision| revision.revision.revision_no).unwrap_or(0) + 1)
    }

    /// 在原执行器取回同一仓库和 SKU 的全部未删除策略，供实体执行完整启用区间重叠校验；查询或游标失败直接返回。
    pub async fn sku_policies_for_dimensions(
        &self,
        warehouse_id: &WarehouseId,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WarehouseSkuPolicy>> {
        mongo_ops::find_many(
            &self.db.collection::<WarehouseSkuPolicy>(WAREHOUSE_SKU_POLICIES),
            doc! {
                "warehouse_id": warehouse_id.to_string(),
                "sku_id": sku_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 先把首修订链接为当前修订，再依次写入仓库和修订，使身份、指针与快照原子可见。调用方必须通过 `Transactional::with_transaction` 提供事务执行器；传 `NoTransaction` 时中途失败会留下没有修订的仓库。
    /// 唯一索引冲突透出 [`persistence_core::Error::DuplicateKey`] 供服务映射，其他 MongoDB 写入失败直接返回。
    pub async fn create_warehouse_with_revision(
        &self,
        warehouse: &mut Warehouse,
        revision: &WarehouseRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        warehouse.stable.current_revision_id = Some(revision.base.id.clone());
        mongo_ops::insert_one(&self.db.collection::<Warehouse>(WAREHOUSES), &*warehouse, executor).await?;
        mongo_ops::insert_one(
            &self.db.collection::<WarehouseRevision>(WAREHOUSE_REVISIONS),
            revision,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 关键词查询单次允许装入 `$in` 的修订 ID 上限（约束单次读取规模）。
const KEYWORD_REVISION_ID_CAP: usize = 500;

// 用相同条件及执行器完成三类列表的投影分页和计数；查询、游标或计数失败直接返回，不交付不完整结果。
async fn search_projected_page<Entity, Row>(
    base: &mongodb::Collection<Entity>,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: i64,
    projection: Document,
    executor: &mut dyn Executor,
) -> Result<PageResult<Row>>
where
    Entity: Send + Sync,
    Row: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    let options = FindOptions::builder().sort(sort).skip(skip).limit(limit).projection(projection).build();
    let collection = base.clone_with_type::<Row>();
    let items = mongo_ops::find_many(&collection, filter.clone(), options, executor).await?;
    let total = mongo_ops::count_documents(base, filter, executor).await?;
    Ok(PageResult { items, total: total as i64 })
}

// 按白名单生成排序，缺省或非法字段回退 `created_at`，追加同向 `id` 打破并列，使分页顺序稳定。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|field| allowed.contains(field)).unwrap_or("created_at");
    doc! { field: direction, "id": direction }
}

// 按仓库代码或当前修订名称匹配关键词，历史修订不算命中。按名称只取未删除修订的 ID；超过 500 项时要求缩小关键词，查询失败直接返回。
async fn keyword_filter(
    repo: &Repository<'_, Warehouse>,
    filter: &WarehouseFilter,
    executor: &mut dyn Executor,
) -> Result<Document> {
    let mut query = filter.to_doc();
    let Some(q) = filter.q.as_deref() else {
        return Ok(query);
    };

    let mut name = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
    insert_literal_regex_filter(&mut name, "name", Some(q));
    let collection = repo.database().collection::<Document>(WAREHOUSE_REVISIONS);
    let ids = mongo_ops::distinct(&collection, "id", name, executor).await?;
    if ids.len() > KEYWORD_REVISION_ID_CAP {
        return Err(mongodb::error::Error::custom("关键词命中修订过多，请使用更精确的关键词").into());
    }

    let mut code = Document::new();
    insert_literal_regex_filter(&mut code, "warehouse_code", Some(q));
    query.insert("$or", vec![code, doc! { "current_revision_id": { "$in": ids } }]);
    Ok(query)
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::{
        WAREHOUSE_SORT_FIELDS, WarehouseFilter, WarehouseRevisionFilter, WarehouseSkuPolicyFilter, sort_doc,
    };
    use crate::entity::warehouse::EnableStatus;

    #[test]
    fn warehouse_filter_applies_optional_fields_and_deleted_filter() {
        let filter = WarehouseFilter {
            warehouse_code: Some("WH-BJ-001".to_string()),
            status: Some(EnableStatus::Active),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("warehouse_code").unwrap(), "WH-BJ-001");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn warehouse_revision_filter_applies_name_regex_and_warehouse_scoping() {
        let filter = WarehouseRevisionFilter {
            warehouse_id: Some("wh-1".to_string()),
            name: Some("北京".to_string()),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("warehouse_id").unwrap(), "wh-1");
        assert!(document.get("name").is_some());
    }

    #[test]
    fn sku_policy_filter_defaults_to_first_page_size_20() {
        let filter = WarehouseSkuPolicyFilter::default();
        assert_eq!((filter.page, filter.page_size), (1, 20));
        assert!(filter.sort_by.is_none());
        assert!(!filter.sort_ascending);
    }

    #[test]
    fn sort_doc_applies_direction() {
        assert_eq!(
            sort_doc(Some("created_at"), false, WAREHOUSE_SORT_FIELDS),
            doc! { "created_at": -1, "id": -1 }
        );
        assert_eq!(
            sort_doc(Some("warehouse_code"), true, WAREHOUSE_SORT_FIELDS),
            doc! { "warehouse_code": 1, "id": 1 }
        );
        assert_eq!(
            sort_doc(Some("任意字段"), false, WAREHOUSE_SORT_FIELDS),
            doc! { "created_at": -1, "id": -1 },
            "白名单外的字段名回落默认排序"
        );
    }
}
