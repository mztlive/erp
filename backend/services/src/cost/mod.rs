//! 域 D20 `cost` 服务编排（页面：W16 实际经营盈亏）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 跨集合写入（成本事实 + 分配行）→
//!   `database::Transactional::with_transaction`；
//! - 列表查询单集合 → `&mut NoTransaction`。
//!
//! 跨域只经 `DatabaseExt` 调对方域 Repository：D13 `sales_order()`
//! 按 ID 集合批量校验成本归属销售单存在（D20 依赖域 D15/D16/D13，本期 P3
//! 只落地 D13 校验与查询编排，D15/D16 的采购/履约来源由对方域在 P3 经
//! `CostExt` 直接写入）。

use database::{AccessControlExt, CostExt, NoTransaction, SalesOrderExt, Transactional};
use entities::cost::{
    CostAllocation, CostAllocationData, CostAllocationLineInput, CostAllocationSet, CostEntry, CostEntryData,
};
use entities::ids::{CostAllocationId, CostEntryId, SalesOrderId};
use id_generator::next_id;
use mongodb::Database;
use std::collections::{HashMap, HashSet};
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
/// 成本事实列表持久化投影类型。
type CostEntryRow = <mongodb::Database as CostExt>::CostEntryRow;
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
        let filter = cost_entry_filter(params)?;
        let page = self
            .db
            .cost_entries()
            .search_cost_entries(&filter, &mut NoTransaction)
            .await?;
        let entry_ids = page
            .items
            .iter()
            .map(|row| CostEntryId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let mut allocations_by_entry = self.cost_allocations_by_entry(&entry_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let allocations = allocations_by_entry.remove(&row.id).unwrap_or_default();
                cost_entry_row_view(row, allocations)
            })
            .collect();
        Ok(PageView {
            items,
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
    /// * `Logic` - 分配合计与事实金额不一致
    pub async fn create_cost_entry(
        &self,
        req: CreateCostEntryRequest,
        actor: &AuditActor,
    ) -> Result<CostEntryView> {
        req.validate()?;
        // 归属销售单存在性：先去重、一次批量读取存在性事实，再解释缺失订单；
        // Repository 只返回已存在 ID 的最小事实，跨聚合报错决策保留 Service。
        let requested_order_ids = req
            .allocations
            .iter()
            .map(|line| line.sales_order_id.clone())
            .collect::<Vec<_>>();
        let unique_order_ids = dedupe_order_ids(&requested_order_ids);
        let existing_order_ids = self
            .db
            .sales_order()
            .find_existing_ids(&unique_order_ids, &mut NoTransaction)
            .await?;
        if missing_order_id(&unique_order_ids, &existing_order_ids).is_some() {
            return Err(Error::NotFound("成本归属销售单不存在".to_string()));
        }
        // 金额守恒与尾差归属由计划 VO 一次性验证与解析，失败不产生部分计划；
        // Service 只注入已确认的销售事实并编排事务。
        let plan = CostAllocationSet::new(
            req.gross_amount,
            req.net_amount,
            req.allocations
                .iter()
                .map(|line| CostAllocationLineInput {
                    sales_order_id: line.sales_order_id.clone(),
                    sales_order_line_id: line.sales_order_line_id.clone(),
                    allocated_gross_amount: line.allocated_gross_amount,
                    allocated_net_amount: line.allocated_net_amount,
                    rounding_residual_flag: line.rounding_residual_flag,
                })
                .collect(),
        )?;

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
        let mut allocations = Vec::with_capacity(plan.lines().len());
        for line in plan.into_lines() {
            allocations.push(CostAllocation::new(
                CostAllocationId::new(next_id()),
                CostAllocationData {
                    cost_entry_id: entry_id.clone(),
                    sales_order_id: Some(line.sales_order_id),
                    sales_order_line_id: line.sales_order_line_id,
                    mall_consumption_entry_id: None,
                    mall_payment_source_id: None,
                    allocated_gross_amount: line.allocated_gross_amount,
                    allocated_net_amount: line.allocated_net_amount,
                    rounding_residual_flag: line.rounding_residual_flag,
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
            .map(cost_allocation_entity_view)
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

    /// 批量读取本页成本事实的分配事实并按成本事实分组。
    ///
    /// # 参数
    /// * `entry_ids` - 当前页成本事实 ID，空页直接得到空映射
    ///
    /// # 返回
    /// 返回按成本事实 ID 分组且保持仓储返回相对顺序的分配事实。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    async fn cost_allocations_by_entry(
        &self,
        entry_ids: &[CostEntryId],
    ) -> Result<HashMap<String, Vec<CostAllocation>>> {
        let allocations = self
            .db
            .cost_allocations()
            .find_allocations_by_entries(entry_ids, &mut NoTransaction)
            .await?;
        let mut by_entry = HashMap::<String, Vec<CostAllocation>>::new();
        for allocation in allocations {
            by_entry
                .entry(allocation.cost_entry_id.to_string())
                .or_default()
                .push(allocation);
        }
        Ok(by_entry)
    }
}

/// 校验并形成成本事实列表仓储筛选条件。
///
/// # 参数
/// * `params` - HTTP 契约复用的列表查询参数
///
/// # 返回
/// 返回白名单排序且分页已归一化的仓储筛选条件。
///
/// # 错误
/// 分页参数或排序字段非法时返回验证错误。
fn cost_entry_filter(params: &CostEntryListParams) -> Result<CostEntryFilter> {
    params.validate()?;
    let query = params.normalized()?;
    Ok(CostEntryFilter {
        cost_type: query.cost_type,
        cost_stage: query.cost_stage,
        cost_scope: query.cost_scope,
        supplier_id: query.supplier_id,
        source_document_id: query.source_document_id,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    })
}

/// 将成本事实投影与已批量加载的分配事实装配为列表视图。
///
/// 金额字段均直接复制持久化金额（`entities::money::Amount`）；分配缺失时保持
/// 既有空列表语义。
///
/// # 参数
/// * `row` - 当前页成本事实持久化投影
/// * `allocations` - 属于该成本事实的持久化分配事实
///
/// # 返回
/// 返回完整成本事实列表视图。
fn cost_entry_row_view(row: CostEntryRow, allocations: Vec<CostAllocation>) -> CostEntryView {
    CostEntryView {
        id: row.id,
        cost_type: row.cost_type,
        cost_stage: row.cost_stage,
        cost_scope: row.cost_scope,
        cost_basis: row.cost_basis,
        supplier_id: row.supplier_id,
        gross_amount: row.gross_amount,
        net_amount: row.net_amount,
        tax_amount: row.tax_amount,
        tax_inclusion: row.tax_inclusion,
        input_tax_rate: row.input_tax_rate,
        occurred_at: row.occurred_at,
        source_fact_type: row.source_fact_type,
        source_document_id: row.source_document_id,
        source_line_id: row.source_line_id,
        source_version: row.source_version,
        created_at: row.created_at,
        allocations: allocations.into_iter().map(cost_allocation_entity_view).collect(),
    }
}

/// 将持久化成本分配事实无损映射为服务响应视图。
///
/// 金额字段直接复制原始金额（`entities::money::Amount`），不得在读取路径归一化、
/// 重算或改变小数位。
///
/// # 参数
/// * `allocation` - 仓储读取的成本分配事实
///
/// # 返回
/// 返回字段一一对应的成本分配响应视图。
fn cost_allocation_entity_view(allocation: CostAllocation) -> CostAllocationView {
    CostAllocationView {
        id: allocation.base.id,
        cost_entry_id: allocation.cost_entry_id.to_string(),
        sales_order_id: allocation.sales_order_id.map(|id| id.to_string()),
        sales_order_line_id: allocation.sales_order_line_id.map(|id| id.to_string()),
        mall_consumption_entry_id: allocation.mall_consumption_entry_id.map(|id| id.to_string()),
        allocated_gross_amount: allocation.allocated_gross_amount,
        allocated_net_amount: allocation.allocated_net_amount,
        rounding_residual_flag: allocation.rounding_residual_flag,
    }
}

/// 去重销售单 ID（保持首次出现顺序）。
///
/// 批量存在性读取前先去重，保证同一销售订单只查询一次。
///
/// # 参数
/// * `ids` - 分配行销售单 ID 列表，可能包含重复
///
/// # 返回
/// 返回去重后的销售单 ID，顺序与首次出现一致。
///
/// # 错误
/// 无。
fn dedupe_order_ids(ids: &[SalesOrderId]) -> Vec<SalesOrderId> {
    let mut seen = HashSet::with_capacity(ids.len());
    ids.iter()
        .filter(|&id| seen.insert(id.clone()))
        .cloned()
        .collect()
}

/// 按输入顺序返回第一个缺失的销售单 ID。
///
/// # 参数
/// * `requested` - 去重后的请求销售单 ID（保持输入顺序）
/// * `existing` - 仓储批量返回的已存在销售单 ID
///
/// # 返回
/// 全部存在时返回 `None`；否则返回输入顺序中第一个缺失的 ID。
///
/// # 错误
/// 无。
fn missing_order_id(requested: &[SalesOrderId], existing: &[SalesOrderId]) -> Option<SalesOrderId> {
    let existing = existing.iter().collect::<HashSet<_>>();
    for id in requested {
        if !existing.contains(id) {
            return Some(id.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        cost_allocation_entity_view, cost_entry_row_view, CostAllocation, CostAllocationData, CostEntryRow,
    };
    use entities::ids::{CostAllocationId, CostEntryId, SalesOrderId};
    use entities::money::Amount;
    use serde_json::json;
    use std::str::FromStr;

    #[test]
    fn entry_view_copies_zero_amount_scale_and_keeps_missing_allocations_empty() {
        let row: CostEntryRow = serde_json::from_value(json!({
            "id": "entry-1",
            "cost_type": "other",
            "cost_stage": "actual",
            "cost_scope": "non_voucher_fulfillment",
            "cost_basis": null,
            "supplier_id": null,
            "gross_amount": "0.00",
            "net_amount": "0.00",
            "tax_amount": "0.00",
            "tax_inclusion": true,
            "input_tax_rate": "0.000000",
            "occurred_at": 1_700_000_000,
            "source_fact_type": "manual",
            "source_document_id": "document-1",
            "source_line_id": "line-1",
            "source_version": "1",
            "version": 1,
            "created_at": 1_700_000_000,
        }))
        .unwrap();

        let view = cost_entry_row_view(row, Vec::new());

        assert_eq!(view.gross_amount.to_string(), "0.00");
        assert_eq!(view.net_amount.to_string(), "0.00");
        assert_eq!(view.tax_amount.to_string(), "0.00");
        assert!(view.allocations.is_empty());
    }

    #[test]
    fn allocation_view_copies_persisted_amount_scale_without_recalculation() {
        let allocation = CostAllocation::new(
            CostAllocationId::new("allocation-1"),
            CostAllocationData {
                cost_entry_id: CostEntryId::new("entry-1"),
                sales_order_id: Some(SalesOrderId::new("sales-order-1")),
                sales_order_line_id: None,
                mall_consumption_entry_id: None,
                mall_payment_source_id: None,
                allocated_gross_amount: Amount::from_str("1.00").unwrap(),
                allocated_net_amount: Amount::from_str("0.10").unwrap(),
                rounding_residual_flag: true,
            },
        )
        .unwrap();

        let view = cost_allocation_entity_view(allocation);

        assert_eq!(view.allocated_gross_amount.to_string(), "1.00");
        assert_eq!(view.allocated_net_amount.to_string(), "0.10");
    }

    #[test]
    fn dedupe_order_ids_keeps_first_occurrence_order() {
        let ids = vec![
            SalesOrderId::new("so-1"),
            SalesOrderId::new("so-2"),
            SalesOrderId::new("so-1"),
            SalesOrderId::new("so-3"),
        ];
        let unique = super::dedupe_order_ids(&ids);
        let strings = unique.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(strings, vec!["so-1", "so-2", "so-3"]);
    }

    #[test]
    fn missing_order_id_returns_none_when_all_exist() {
        let requested = vec![SalesOrderId::new("so-1"), SalesOrderId::new("so-2")];
        let existing = vec![SalesOrderId::new("so-2"), SalesOrderId::new("so-1")];
        assert!(super::missing_order_id(&requested, &existing).is_none());
    }

    #[test]
    fn missing_order_id_reports_first_missing_in_input_order() {
        let requested = vec![SalesOrderId::new("so-1"), SalesOrderId::new("so-2")];
        let existing = vec![SalesOrderId::new("so-1")];
        assert_eq!(
            super::missing_order_id(&requested, &existing).unwrap(),
            SalesOrderId::new("so-2")
        );

        let empty_existing = Vec::<SalesOrderId>::new();
        assert_eq!(
            super::missing_order_id(&requested, &empty_existing).unwrap(),
            SalesOrderId::new("so-1")
        );
    }

    #[test]
    fn missing_order_id_empty_request_is_none() {
        assert!(super::missing_order_id(&[], &[SalesOrderId::new("so-1")]).is_none());
    }
}
