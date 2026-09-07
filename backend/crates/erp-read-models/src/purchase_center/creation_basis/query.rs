use super::super::repository::{
    load_creation_basis_facts, load_sales_procurement_coverage, sales_order_basis_fact,
    stock_basis_groups_for_order,
};
use super::super::{
    dto::{CreationBasisListParams, CreationBasisView},
    PurchaseOrderReadService,
};
use super::mapping::{build_basis_view, build_stock_basis_view};
use application_core::AuditActor;
use erp_core::ids::{SalesOrderId, SkuId};
use erp_procurement::entity::purchase_order::SalesProcurementCoverage;
use erp_procurement::service::purchase_order::creation_basis::{basis_groups_from_facts, zero_quantity};
use erp_sales::repository::SalesOrderExt;
use erp_workflow::WorkItemExt;
use persistence_core::NoTransaction;
use services::{Error, Result};
use std::collections::{HashMap, HashSet};
impl PurchaseOrderReadService {
    /// 查询当前账号开放采购任务范围内仍有剩余量的精确采购创建依据。
    ///
    /// # 参数
    /// * `params` - 可选销售单与供给分配任务筛选
    /// * `actor` - 当前已认证账号
    ///
    /// # 返回
    /// 返回按具体开放任务、销售单和精确拆分维度形成的创建依据。
    ///
    /// # 错误
    /// 当前销售版本、采购覆盖、任务范围或供应商供给数据不一致，以及仓储查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只展示当前账号拥有的开放任务冻结行；客户端不能看到或创建其他采购负责人的范围。
    /// 供给、修订、可供投影与供应商结算事实按全部任务涉及 SKU 一次批量读取，
    /// 查询次数不随任务数、销售行数或供给数线性增长。
    pub async fn creation_basis_list(
        &self,
        params: &CreationBasisListParams,
        actor: &AuditActor,
    ) -> Result<Vec<CreationBasisView>> {
        let sales_order_id = normalized_optional_filter(params.sales_order_id.as_deref());
        let work_item_id = normalized_optional_filter(params.work_item_id.as_deref());
        let tasks = self
            .db
            .work_items()
            .list_open_procurement_owned_by(
                actor.id(),
                sales_order_id.as_deref(),
                work_item_id.as_deref(),
                &mut NoTransaction,
            )
            .await?;
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        let order_ids = tasks
            .iter()
            .map(|task| SalesOrderId::new(task.business_object_id.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let orders = self
            .db
            .sales_orders()
            .find_effective_orders_by_ids(&order_ids, &mut NoTransaction)
            .await?;
        let owner_names = self
            .resolve_account_names(
                &orders
                    .iter()
                    .map(|order| order.stable.created_by.clone())
                    .collect::<Vec<_>>(),
            )
            .await?;
        let orders = orders
            .into_iter()
            .map(|order| (order.base.id.clone(), order))
            .collect::<HashMap<_, _>>();
        // 按销售单归组任务，使覆盖与供给事实按单一次批量读取。
        let mut task_indexes_by_order: Vec<(String, Vec<usize>)> = Vec::new();
        for (index, task) in tasks.iter().enumerate() {
            if task.responsibility_key().is_none() || task.responsibility_scope_ids().is_empty() {
                return Err(Error::ConflictError("供给分配任务缺少冻结责任范围".to_string()));
            }
            if let Some(entry) = task_indexes_by_order
                .iter_mut()
                .find(|(order_id, _)| *order_id == task.business_object_id)
            {
                entry.1.push(index);
            } else {
                task_indexes_by_order.push((task.business_object_id.clone(), vec![index]));
            }
        }
        // 每张销售单一次覆盖读取；全部任务共享同一批供给与结算事实。
        let mut coverage_by_order: HashMap<String, SalesProcurementCoverage> = HashMap::new();
        let mut all_sku_ids: Vec<SkuId> = Vec::new();
        for (order_id, task_indexes) in &task_indexes_by_order {
            let Some(order) = orders.get(order_id) else {
                continue;
            };
            let coverage = load_sales_procurement_coverage(&self.db, order, &mut NoTransaction).await?;
            for &task_index in task_indexes {
                let scope = tasks[task_index]
                    .responsibility_scope_ids()
                    .iter()
                    .map(String::as_str)
                    .collect::<HashSet<_>>();
                for line in &coverage.lines {
                    if scope.contains(line.revision_line.sales_order_line_id.as_ref())
                        && line.summary.remaining_quantity > zero_quantity()
                    {
                        all_sku_ids.push(line.goods_line.sku_id.clone());
                    }
                }
            }
            coverage_by_order.insert(order_id.clone(), coverage);
        }
        let facts = load_creation_basis_facts(&self.db, &all_sku_ids, &mut NoTransaction).await?;
        let mut views = Vec::new();
        for (order_id, task_indexes) in task_indexes_by_order {
            let Some(order) = orders.get(&order_id) else {
                continue;
            };
            let coverage = coverage_by_order
                .get(&order_id)
                .expect("已加载的销售覆盖必须存在");
            let owner_name = owner_names.get(&order.stable.created_by).cloned();
            for task_index in task_indexes {
                let task = &tasks[task_index];
                let groups = basis_groups_from_facts(
                    &sales_order_basis_fact(order),
                    coverage,
                    task.responsibility_scope_ids(),
                    &facts,
                )?;
                for group in groups {
                    views.push(build_basis_view(
                        &sales_order_basis_fact(order),
                        &group,
                        &facts,
                        owner_name.clone(),
                        &task.base.id,
                    )?);
                }
                let stock_groups = stock_basis_groups_for_order(
                    &self.db,
                    order,
                    task.responsibility_scope_ids(),
                    &mut NoTransaction,
                )
                .await?;
                for group in stock_groups {
                    views.push(build_stock_basis_view(
                        &sales_order_basis_fact(order),
                        &group,
                        owner_name.clone(),
                        &task.base.id,
                    )?);
                }
            }
        }
        Ok(views)
    }
}

/// 将可选查询文本规范化为空或去除首尾空白后的值。
///
/// # 参数
/// * `value` - 可选原始查询文本
///
/// # 返回
/// 空白值返回 `None`，否则返回规范化字符串。
///
/// # 错误
/// 无。
fn normalized_optional_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
