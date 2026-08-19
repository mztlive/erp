//! 域 D20 `cost` 仓储：cost_entry、cost_allocation。
//!
//! 单一集合 CRUD 直接复用 [`Repository`] 基类；本文件只补充域特有查询与
//! 跨集合多步骤事务写入入口。集合名常量统一从 `indexes::cost` 导入。
//!
//! 成本事实与成本分配是正式事实集合（§4.5），过账后不可更新或删除，
//! **不提供软删除方法**（冲减用 `REDUCTION` 阶段追加事实）。
//!
//! 筛选/行类型定义在本文件，经 `CostExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::cost::{CostAllocation, CostBasis, CostEntry, CostScope, CostStage, CostType};
use entities::ids::{CostEntryId, MallConsumptionEntryId, SalesOrderId, SupplierAccountId};
use entities::money::Amount;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::CostExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `cost_allocation` 集合名（单一来源：`CostExt` 关联常量）。
const COST_ALLOCATIONS: &str = <mongodb::Database as CostExt>::COST_ALLOCATIONS;

/// 成本事实列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntryRow {
    /// 实体主键。
    pub id: String,
    /// 成本类型。
    pub cost_type: CostType,
    /// 成本阶段。
    pub cost_stage: CostStage,
    /// 成本归属范围。
    pub cost_scope: CostScope,
    /// 成本取值基础。
    pub cost_basis: Option<CostBasis>,
    /// 成本供应商。
    pub supplier_id: Option<String>,
    /// 含税成本金额。
    pub gross_amount: Amount,
    /// 不含税成本金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 成本发生时间（秒级时间戳）。
    pub occurred_at: u64,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 成本事实列表筛选条件。
#[derive(Debug, Clone)]
pub struct CostEntryFilter {
    /// 成本类型；`None` 表示不筛选。
    pub cost_type: Option<CostType>,
    /// 成本阶段；`None` 表示不筛选。
    pub cost_stage: Option<CostStage>,
    /// 成本归属范围；`None` 表示不筛选。
    pub cost_scope: Option<CostScope>,
    /// 成本供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 来源单据 ID 模糊匹配；`None` 表示不筛选。
    pub source_document_id: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for CostEntryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(cost_type) = self.cost_type {
            filter.insert("cost_type", cost_type.as_str());
        }
        if let Some(cost_stage) = self.cost_stage {
            filter.insert("cost_stage", cost_stage.as_str());
        }
        if let Some(cost_scope) = self.cost_scope {
            filter.insert("cost_scope", cost_scope.as_str());
        }
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        insert_literal_regex_filter(
            &mut filter,
            "source_document_id",
            self.source_document_id.as_deref(),
        );
        filter
    }
}

impl Pagination for CostEntryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 成本分配列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAllocationRow {
    /// 实体主键。
    pub id: String,
    /// 成本事实。
    pub cost_entry_id: String,
    /// 经营归属销售单。
    pub sales_order_id: Option<String>,
    /// 经营归属销售明细。
    pub sales_order_line_id: Option<String>,
    /// 二期消费成本归属。
    pub mall_consumption_entry_id: Option<String>,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 是否承担最后一笔尾差。
    pub rounding_residual_flag: bool,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 成本分配列表筛选条件。
#[derive(Debug, Clone)]
pub struct CostAllocationFilter {
    /// 成本事实；`None` 表示不筛选。
    pub cost_entry_id: Option<CostEntryId>,
    /// 经营归属销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 二期消费成本归属；`None` 表示不筛选。
    pub mall_consumption_entry_id: Option<MallConsumptionEntryId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for CostAllocationFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(cost_entry_id) = &self.cost_entry_id {
            filter.insert("cost_entry_id", cost_entry_id.to_string());
        }
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(mall_consumption_entry_id) = &self.mall_consumption_entry_id {
            filter.insert("mall_consumption_entry_id", mall_consumption_entry_id.to_string());
        }
        filter
    }
}

impl Pagination for CostAllocationFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, CostEntry> {
    /// 分页检索成本事实列表（投影查询）。
    ///
    /// 只返回 [`CostEntryRow`] 所需的列表字段；来源单据 ID 支持字面量模糊匹配
    /// （复用 `regex_filter`，禁止自拼正则）。
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
    pub async fn search_cost_entries(
        &self,
        filter: &CostEntryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CostEntryRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["occurred_at", "gross_amount", "net_amount", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(cost_entry_projection())
            .build();
        let collection = self.collection().clone_with_type::<CostEntryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, CostAllocation> {
    /// 分页检索成本分配列表（投影查询）。
    ///
    /// 只返回 [`CostAllocationRow`] 所需的列表字段；支持按成本事实、销售单
    /// 与二期消费归集筛选。
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
    pub async fn search_cost_allocations(
        &self,
        filter: &CostAllocationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CostAllocationRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["allocated_gross_amount", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(cost_allocation_projection())
            .build();
        let collection = self.collection().clone_with_type::<CostAllocationRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 批量按成本事实集合取回分配（`$in` 一次取回，禁止 N+1）。
    ///
    /// 用于校验「成本分配合计等于成本事实金额」（数据模型 §6.10）。
    ///
    /// # 参数
    /// * `entry_ids` - 成本事实 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_entries(
        &self,
        entry_ids: &[CostEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocation>> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let entry_ids: Vec<String> = entry_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "cost_entry_id": { "$in": entry_ids } }, executor)
            .await
    }
}

/// D20 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `CostExt::cost()` 访问。
pub struct CostRepository<'a> {
    db: &'a Database,
}

impl<'a> CostRepository<'a> {
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

    /// 建立成本事实与其分配行（跨集合多步骤写入）。
    ///
    /// 依次写入 `cost_entries` 与 `cost_allocations`，保证「成本事实 + 分配行」
    /// 原子可见（数据模型 §6.10 成本分配合计等于成本事实金额）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 各笔写入自动提交，后续分配失败会留下没有分配行的成本事实；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `entry` - 待写入的成本事实
    /// * `allocations` - 待写入的分配行集合（可为空）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_cost_entry_with_allocations(
        &self,
        entry: &CostEntry,
        allocations: Vec<CostAllocation>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<CostEntry>(<mongodb::Database as CostExt>::COST_ENTRIES),
            entry,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<CostAllocation>(COST_ALLOCATIONS),
            allocations,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档：字段名经白名单映射，未命中回退 `created_at` 降序。
///
/// # 参数
/// * `sort_by` - 排序字段（白名单内有效）
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段名集合（防止透传任意字段名）
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|name| allowed.contains(name))
        .unwrap_or("created_at");
    doc! { field: direction }
}

/// 成本事实列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn cost_entry_projection() -> Document {
    doc! {
        "id": 1,
        "cost_type": 1,
        "cost_stage": 1,
        "cost_scope": 1,
        "cost_basis": 1,
        "supplier_id": 1,
        "gross_amount": 1,
        "net_amount": 1,
        "tax_amount": 1,
        "occurred_at": 1,
        "source_document_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 成本分配列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn cost_allocation_projection() -> Document {
    doc! {
        "id": 1,
        "cost_entry_id": 1,
        "sales_order_id": 1,
        "sales_order_line_id": 1,
        "mall_consumption_entry_id": 1,
        "allocated_gross_amount": 1,
        "allocated_net_amount": 1,
        "rounding_residual_flag": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, CostAllocationFilter, CostEntryFilter, QueryFilter};
    use entities::cost::{CostScope, CostStage, CostType};
    use entities::ids::SupplierAccountId;
    use mongodb::bson::doc;

    #[test]
    fn entry_filter_applies_optional_fields_and_deleted_filter() {
        let filter = CostEntryFilter {
            cost_type: Some(CostType::Product),
            cost_stage: Some(CostStage::Actual),
            cost_scope: Some(CostScope::NonVoucherFulfillment),
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            source_document_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("cost_type").unwrap(), "product");
        assert_eq!(document.get_str("cost_stage").unwrap(), "actual");
        assert_eq!(document.get_str("cost_scope").unwrap(), "non_voucher_fulfillment");
        assert_eq!(document.get_str("supplier_id").unwrap(), "sup-1");
    }

    #[test]
    fn allocation_filter_escapes_regex_and_filters_by_target() {
        let filter = CostAllocationFilter {
            cost_entry_id: None,
            sales_order_id: Some(entities::ids::SalesOrderId::new("so-1")),
            mall_consumption_entry_id: None,
            page: 1,
            page_size: 20,
            sort_by: Some("$where".to_string()),
            sort_ascending: true,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("sales_order_id").unwrap(), "so-1");
        assert_eq!(
            sort_doc(filter.sort_by.as_deref(), true, &["allocated_gross_amount"]),
            doc! { "created_at": 1 }
        );
    }
}
