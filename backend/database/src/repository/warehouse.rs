//! 域 D11 `warehouse` 仓储：warehouse、warehouse_revision、warehouse_sku_policy。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充域特有
//! 查询与跨集合多步骤写入入口。集合名常量统一从 `WarehouseExt` 关联常量导入。
//!
//! `warehouse_revision` 与 `warehouse_sku_policy` 的敏感字段（地址/联系人加密列）
//! 不进入任何列表投影；`warehouse_revision` 是追加写入修订，不提供软删除方法。
//!
//! 筛选/行类型定义在本文件，经 `WarehouseExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::WarehouseExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

use entities::common::time::BusinessDate;
use entities::ids::{SkuId, WarehouseId};
use entities::money::Quantity;
use entities::warehouse::{EnableStatus, Warehouse, WarehouseRevision, WarehouseSkuPolicy};

/// `warehouse` 集合名（单一来源：`WarehouseExt` 关联常量）。
const WAREHOUSES: &str = <mongodb::Database as WarehouseExt>::WAREHOUSES;
/// `warehouse_revision` 集合名（单一来源：`WarehouseExt` 关联常量）。
const WAREHOUSE_REVISIONS: &str = <mongodb::Database as WarehouseExt>::WAREHOUSE_REVISIONS;
/// `warehouse_sku_policy` 集合名。
const WAREHOUSE_SKU_POLICIES: &str = <mongodb::Database as WarehouseExt>::WAREHOUSE_SKU_POLICIES;

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

impl QueryFilter for WarehouseFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(code) = &self.warehouse_code {
            filter.insert("warehouse_code", code);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for WarehouseFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, Warehouse> {
    /// 分页检索仓库列表（投影查询）。
    ///
    /// 只返回 [`WarehouseRow`] 所需的列表字段，不加载整文档；
    /// 排序字段白名单化（`created_at`/`warehouse_code`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_warehouses(
        &self,
        filter: &WarehouseFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WarehouseRow>> {
        let options = FindOptions::builder()
            .sort(warehouse_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(warehouse_projection())
            .build();
        let collection = self.collection().clone_with_type::<WarehouseRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
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

impl QueryFilter for WarehouseRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订表不参与软删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(warehouse_id) = &self.warehouse_id {
            filter.insert("warehouse_id", warehouse_id);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        filter
    }
}

impl Pagination for WarehouseRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, WarehouseRevision> {
    /// 分页检索仓库修订列表（投影查询）。
    ///
    /// 只返回 [`WarehouseRevisionRow`] 所需的列表字段，**不返回加密地址与
    /// 联系人**；排序字段白名单化（`created_at`/`revision_no`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_warehouse_revisions(
        &self,
        filter: &WarehouseRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WarehouseRevisionRow>> {
        let options = FindOptions::builder()
            .sort(warehouse_revision_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(warehouse_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<WarehouseRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
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
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
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

impl Pagination for WarehouseSkuPolicyFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, WarehouseSkuPolicy> {
    /// 分页检索仓库-SKU 预警策略列表（投影查询）。
    ///
    /// 只返回 [`WarehouseSkuPolicyRow`] 所需的列表字段（含 Decimal128 预警阈值，
    /// 不做舍入或换算）；排序字段白名单化（`created_at`/`effective_from`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_warehouse_sku_policies(
        &self,
        filter: &WarehouseSkuPolicyFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WarehouseSkuPolicyRow>> {
        let options = FindOptions::builder()
            .sort(warehouse_sku_policy_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(warehouse_sku_policy_projection())
            .build();
        let collection = self.collection().clone_with_type::<WarehouseSkuPolicyRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 批量查询一组 SKU 的仓库预警策略（`$in`，一次取回）。
    ///
    /// 用于按 SKU 聚合策略明细，避免逐 SKU N+1。
    ///
    /// # 参数
    /// * `sku_ids` - SKU ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除策略实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_sku_ids(
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
pub struct WarehouseRepository<'a> {
    db: &'a Database,
}

impl<'a> WarehouseRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 按主键读取未删除仓库。
    ///
    /// # 参数
    /// * `id` - 仓库主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配仓库；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn warehouse(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<Warehouse>> {
        active_entity_by_id(self.db, WAREHOUSES, id, executor).await
    }

    /// 按主键读取未删除仓库-SKU 策略。
    ///
    /// # 参数
    /// * `id` - 策略主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配策略；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn sku_policy(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WarehouseSkuPolicy>> {
        active_entity_by_id(self.db, WAREHOUSE_SKU_POLICIES, id, executor).await
    }

    /// 计算仓库下一个修订序号。
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已有最大修订序号加一；没有修订时返回 `1`。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn next_revision_no(&self, warehouse_id: &str, executor: &mut dyn Executor) -> Result<u32> {
        let revisions = mongo_ops::find_many(
            &self.db.collection::<WarehouseRevision>(WAREHOUSE_REVISIONS),
            doc! {
                "warehouse_id": warehouse_id,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .sort(doc! { "revision_no": -1 })
                .limit(1)
                .build(),
            executor,
        )
        .await?;
        Ok(revisions
            .first()
            .map(|revision| revision.revision.revision_no)
            .unwrap_or(0)
            + 1)
    }

    /// 读取同一仓库与 SKU 的全部未删除策略。
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库主键
    /// * `sku_id` - SKU 主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回同维度策略，供实体执行启用区间重叠校验。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
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

    /// 建立「仓库稳定身份 + 首个仓库修订」（跨集合多步骤写入）。
    ///
    /// 先把首个修订 ID 链入 `warehouse.stable.current_revision_id`，再依次写入
    /// `warehouses` 与 `warehouse_revisions`，保证「仓库身份 + 当前修订指针 +
    /// 修订快照」原子可见（数据模型 §6.3）。**必须收到事务执行器**：本方法
    /// 不构成原子边界，传入 `NoTransaction` 时两笔写入各自自动提交，中途失败
    /// 会留下没有修订的仓库；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `warehouse` - 待写入的仓库（首修订链入后写库）
    /// * `revision` - 待写入的仓库首个修订
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_warehouse_with_revision(
        &self,
        warehouse: &mut Warehouse,
        revision: &WarehouseRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        warehouse.stable.current_revision_id = Some(revision.base.id.clone());
        mongo_ops::insert_one(
            &self.db.collection::<Warehouse>(WAREHOUSES),
            &*warehouse,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<WarehouseRevision>(WAREHOUSE_REVISIONS),
            revision,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 按主键读取未删除实体。
///
/// # 参数
/// * `db` - 数据库
/// * `collection_name` - 集合名
/// * `id` - 实体主键
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回匹配实体；不存在时返回 `None`。
///
/// # 错误
/// MongoDB 查询失败时返回错误。
async fn active_entity_by_id<T>(
    db: &Database,
    collection_name: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<T>>
where
    T: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    mongo_ops::find_one(
        &db.collection::<T>(collection_name),
        doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
        executor,
    )
    .await
}

/// 构建排序文档。
///
/// # 参数
/// * `field` - 已通过白名单校验的排序字段
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(field: &str, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction }
}

/// 构建仓库排序文档（白名单：`created_at`/`warehouse_code`）。
fn warehouse_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("warehouse_code") => "warehouse_code",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建仓库修订排序文档（白名单：`created_at`/`revision_no`）。
fn warehouse_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("revision_no") => "revision_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建仓库-SKU 预警策略排序文档（白名单：`created_at`/`effective_from`）。
fn warehouse_sku_policy_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("effective_from") => "effective_from",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 仓库列表投影字段。
fn warehouse_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_code": 1,
        "status": 1,
        "inbound_handler_user_id": 1,
        "outbound_handler_user_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 仓库修订列表投影字段（敏感字段密文/指纹不进入投影）。
fn warehouse_revision_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_id": 1,
        "revision_no": 1,
        "name": 1,
        "effective_from": 1,
        "effective_to": 1,
        "change_reason": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 仓库-SKU 预警策略列表投影字段。
fn warehouse_sku_policy_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_id": 1,
        "sku_id": 1,
        "minimum_available_quantity": 1,
        "status": 1,
        "effective_from": 1,
        "effective_to": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, QueryFilter, WarehouseFilter, WarehouseRevisionFilter};
    use entities::warehouse::EnableStatus;
    use mongodb::bson::doc;

    #[test]
    fn warehouse_filter_applies_optional_fields_and_deleted_filter() {
        let filter = WarehouseFilter {
            warehouse_code: Some("WH-BJ-001".to_string()),
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
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
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("warehouse_id").unwrap(), "wh-1");
        assert!(document.get("name").is_some());
    }

    #[test]
    fn sort_doc_applies_direction() {
        assert_eq!(sort_doc("created_at", false), doc! { "created_at": -1 });
        assert_eq!(sort_doc("warehouse_code", true), doc! { "warehouse_code": 1 });
    }
}
