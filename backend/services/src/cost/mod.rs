//! 域 D20 `cost` 服务编排（页面：W16 实际经营盈亏）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 跨集合写入（成本事实 + 分配行）→
//!   `database::Transactional::with_transaction`；
//! - 列表查询单集合 → `&mut NoTransaction`。
//!
//! 跨域只经 `DatabaseExt` 调对方域 Repository：D13 `sales_orders()` 校验
//! 成本归属销售单存在（D20 依赖域 D15/D16/D13，本期 P3 只落地 D13 校验与
//! 查询编排，D15/D16 的采购/履约来源由对方域在 P3 经 `CostExt` 直接写入）。

use database::{AccessControlExt, CostExt, NoTransaction, SalesOrderExt, Transactional};
use entities::cost::{CostAllocation, CostAllocationData, CostEntry, CostEntryData};
use entities::ids::{CostAllocationId, CostEntryId};
use entities::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    CostAllocationListParams, CostAllocationView, CostEntryListParams, CostEntryView, CreateCostEntryRequest,
    PageView,
};

/// 成本事实列表筛选条件类型（经 `CostExt` 关联类型跨 crate 可达）。
type CostEntryFilter = <mongodb::Database as CostExt>::CostEntryFilter;
/// 成本分配列表筛选条件类型。
type CostAllocationFilter = <mongodb::Database as CostExt>::CostAllocationFilter;

/// 成本服务。
///
/// 提供成本事实与成本分配的查询与手工入账编排。
pub struct CostService {
    db: Database,
}

impl CostService {
    /// 创建成本服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询成本事实列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`cost_type`/`cost_stage`/`cost_scope`/`supplier_id`/
    ///   `source_document_id` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn cost_entry_list(&self, params: &CostEntryListParams) -> Result<PageView<CostEntryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CostEntryFilter {
            cost_type: query.cost_type,
            cost_stage: query.cost_stage,
            cost_scope: query.cost_scope,
            supplier_id: query.supplier_id,
            source_document_id: query.source_document_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .cost_entries()
            .search_cost_entries(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.cost_entry_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询成本事实详情（事实 + 分配行）。
    ///
    /// # 参数
    /// * `id` - 成本事实 ID
    ///
    /// # 返回
    /// 返回完整成本视图。
    ///
    /// # 错误
    /// * `NotFound` - 成本事实不存在
    pub async fn cost_entry_detail(&self, id: &str) -> Result<CostEntryView> {
        self.cost_entry_view(id.to_string()).await
    }

    /// 手工登记成本事实与分配行（跨集合事务写入）。
    ///
    /// 同一事务内：校验归属销售单存在（D13 Repository）；分配合计必须等于
    /// 成本事实金额（含税与不含税双侧）；写入成本事实与分配行，保证「事实 +
    /// 分配行」原子可见（数据模型 §6.10）。业务幂等唯一
    /// `(source_fact_type, source_document_id, source_line_id, source_version,
    /// cost_stage, cost_type)` 由唯一索引保证，重复提交落入 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建成本事实的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 归属销售单不存在
    /// * `ConflictError` - 业务唯一键重复
    /// * `BusinessLogicError` - 分配合计与事实金额不一致
    pub async fn create_cost_entry(
        &self,
        req: CreateCostEntryRequest,
        actor: &AuditActor,
    ) -> Result<CostEntryView> {
        req.validate()?;
        for line in &req.allocations {
            self.db
                .sales_orders()
                .find_by_id(&line.sales_order_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("成本归属销售单不存在".to_string()))?;
        }
        let gross_requested: Amount = req.allocations.iter().fold(zero_amount(), |sum, line| {
            sum.checked_add(line.allocated_gross_amount)
        });
        let net_requested: Amount = req.allocations.iter().fold(zero_amount(), |sum, line| {
            sum.checked_add(line.allocated_net_amount)
        });
        if gross_requested != req.gross_amount || net_requested != req.net_amount {
            return Err(Error::BusinessLogicError(
                "成本分配合计必须等于成本事实金额".to_string(),
            ));
        }

        let entry_id = CostEntryId::new(next_id());
        let entry = CostEntry::new(
            entry_id.clone(),
            CostEntryData {
                cost_type: req.cost_type,
                cost_stage: req.cost_stage,
                cost_scope: req.cost_scope,
                cost_basis: req.cost_basis,
                supplier_id: req.supplier_id,
                gross_amount: req.gross_amount,
                net_amount: req.net_amount,
                tax_amount: req.tax_amount,
                tax_inclusion: req.tax_inclusion,
                input_tax_rate: req.input_tax_rate,
                occurred_at: req.occurred_at,
                source_fact_type: req.source_fact_type,
                source_document_id: req.source_document_id,
                source_line_id: req.source_line_id,
                source_version: req.source_version,
                adjusts_cost_entry_id: None,
                evidence_attachment_id: req.evidence_attachment_id,
            },
        )?;
        let mut allocations = Vec::with_capacity(req.allocations.len());
        for (index, line) in req.allocations.iter().enumerate() {
            allocations.push(CostAllocation::new(
                CostAllocationId::new(next_id()),
                CostAllocationData {
                    cost_entry_id: entry_id.clone(),
                    sales_order_id: Some(line.sales_order_id.clone()),
                    sales_order_line_id: line.sales_order_line_id.clone(),
                    mall_consumption_entry_id: None,
                    mall_payment_source_id: None,
                    allocated_gross_amount: line.allocated_gross_amount,
                    allocated_net_amount: line.allocated_net_amount,
                    rounding_residual_flag: line.rounding_residual_flag.unwrap_or(index == 0),
                },
            )?);
        }
        let audit = actor
            .clone()
            .resource_log("cost_entry.create", "cost_entry", entry_id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.cost()
                        .create_cost_entry_with_allocations(&entry, allocations, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.cost_entry_detail(&entry_id).await
    }

    /// 分页查询成本分配列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`cost_entry_id`/`sales_order_id` 筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn cost_allocation_list(
        &self,
        params: &CostAllocationListParams,
    ) -> Result<PageView<CostAllocationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CostAllocationFilter {
            cost_entry_id: query.cost_entry_id,
            sales_order_id: query.sales_order_id,
            mall_consumption_entry_id: None,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .cost_allocations()
            .search_cost_allocations(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
        // 此处按字段映射为响应视图，避免把仓储类型泄漏到接口层。
        let items = page
            .items
            .into_iter()
            .map(|row| CostAllocationView {
                id: row.id,
                cost_entry_id: row.cost_entry_id,
                sales_order_id: row.sales_order_id,
                sales_order_line_id: row.sales_order_line_id,
                mall_consumption_entry_id: row.mall_consumption_entry_id,
                allocated_gross_amount: row.allocated_gross_amount,
                allocated_net_amount: row.allocated_net_amount,
                rounding_residual_flag: row.rounding_residual_flag,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 装配成本事实详情视图。
    ///
    /// # 参数
    /// * `id` - 成本事实 ID
    ///
    /// # 返回
    /// 返回完整成本视图。
    ///
    /// # 错误
    /// * `NotFound` - 成本事实不存在
    async fn cost_entry_view(&self, id: String) -> Result<CostEntryView> {
        let entry = self
            .db
            .cost_entries()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("成本事实不存在".to_string()))?;
        let allocations = self
            .db
            .cost_allocations()
            .find_allocations_by_entries(&[entry.base.id.clone().into()], &mut NoTransaction)
            .await?
            .into_iter()
            .map(|allocation| CostAllocationView {
                id: allocation.base.id.clone(),
                cost_entry_id: allocation.cost_entry_id.to_string(),
                sales_order_id: allocation.sales_order_id.map(|id| id.to_string()),
                sales_order_line_id: allocation.sales_order_line_id.map(|id| id.to_string()),
                mall_consumption_entry_id: allocation.mall_consumption_entry_id.map(|id| id.to_string()),
                allocated_gross_amount: allocation.allocated_gross_amount,
                allocated_net_amount: allocation.allocated_net_amount,
                rounding_residual_flag: allocation.rounding_residual_flag,
            })
            .collect();
        Ok(CostEntryView {
            id: entry.base.id.clone(),
            cost_type: entry.cost_type,
            cost_stage: entry.cost_stage,
            cost_scope: entry.cost_scope,
            cost_basis: entry.cost_basis,
            supplier_id: entry.supplier_id.map(|id| id.to_string()),
            gross_amount: entry.gross_amount,
            net_amount: entry.net_amount,
            tax_amount: entry.tax_amount,
            tax_inclusion: entry.tax_inclusion,
            input_tax_rate: entry.input_tax_rate,
            occurred_at: entry.occurred_at,
            source_fact_type: entry.source_fact_type,
            source_document_id: entry.source_document_id,
            source_line_id: entry.source_line_id,
            source_version: entry.source_version,
            created_at: entry.base.created_at,
            allocations,
        })
    }
}

/// 返回固定零金额。
///
/// # 返回
/// 返回金额 `0.00`。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}
