use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_core::ids::{SalesOrderId, SkuId};
use erp_procurement::entity::purchase_order::{CreationBasisFacts, SalesProcurementCoverage};
use erp_procurement::service::purchase_order::creation_basis::zero_quantity;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use persistence_core::NoTransaction;

use super::super::PurchaseOrderReadService;
use super::super::dto::{CreationBasisListParams, CreationBasisView};
use super::super::repository::{
    InquiryBasis, WarehouseRegionFact, active_warehouse_regions, load_creation_basis_facts,
    load_sales_procurement_coverage, sales_order_basis_fact, stock_basis_groups_for_order,
};
use super::mapping::{build_basis_view, build_stock_basis_view};
use crate::{Error, Result};
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
        let list = BasisList::load(self, tasks, actor).await?;
        list.views(&self.db).await
    }
}

/// 本次授权查询的批量事实；询价信息仍按每张销售单分别装载。
struct BasisList {
    tasks: Vec<WorkItem>,
    orders: HashMap<String, SalesOrder>,
    owner_names: HashMap<String, String>,
    task_indexes: Vec<(String, Vec<usize>)>,
    coverages: HashMap<String, SalesProcurementCoverage>,
    facts: CreationBasisFacts,
    warehouses: Vec<WarehouseRegionFact>,
}

impl BasisList {
    /// 按已授权任务批量装载销售、覆盖及供给，不按行重复查询供给。
    async fn load(
        service: &PurchaseOrderReadService,
        tasks: Vec<WorkItem>,
        actor: &AuditActor,
    ) -> Result<Self> {
        let task_indexes = group_tasks(&tasks, actor)?;
        let ids = task_indexes.iter().map(|(id, _)| SalesOrderId::new(id.clone())).collect::<Vec<_>>();
        let orders = service.db.sales_orders().find_effective_orders_by_ids(&ids, &mut NoTransaction).await?;
        let owner_names = service
            .resolve_account_names(
                &orders.iter().map(|order| order.stable.created_by.clone()).collect::<Vec<_>>(),
            )
            .await?;
        let orders = orders.into_iter().map(|order| (order.base.id.clone(), order)).collect();
        let (coverages, sku_ids) = load_coverages(&service.db, &orders, &tasks, &task_indexes).await?;
        let facts = load_creation_basis_facts(&service.db, &sku_ids, &mut NoTransaction).await?;
        let warehouses = active_warehouse_regions(&service.db, &mut NoTransaction).await?;
        Ok(Self { tasks, orders, owner_names, task_indexes, coverages, facts, warehouses })
    }

    /// 每张销售单加载一次询价来源，交由全部所属任务复用。
    async fn views(mut self, db: &Database) -> Result<Vec<CreationBasisView>> {
        let mut views = Vec::new();
        for (order_id, indexes) in &self.task_indexes {
            let Some(order) = self.orders.get(order_id) else { continue };
            let coverage = self
                .coverages
                .get(order_id)
                .ok_or_else(|| Error::ConflictError("销售覆盖资料已变化，请重新加载".into()))?;
            let inquiry = InquiryBasis::load(db, order, &mut NoTransaction).await?;
            let owner_name = self.owner_names.get(&order.stable.created_by).cloned();
            for &index in indexes {
                let task = &self.tasks[index];
                let groups = inquiry.groups(
                    &sales_order_basis_fact(order),
                    coverage,
                    task.responsibility_scope_ids(),
                    &mut self.facts,
                )?;
                for group in groups {
                    views.push(build_basis_view(
                        &sales_order_basis_fact(order),
                        &group,
                        &self.facts,
                        owner_name.clone(),
                        &task.base.id,
                        &self.warehouses,
                    )?);
                }
                let stock_groups = stock_basis_groups_for_order(
                    db,
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

/// 重验每个任务的负责人及冻结责任范围，保持任务原有顺序。
fn group_tasks(tasks: &[WorkItem], actor: &AuditActor) -> Result<Vec<(String, Vec<usize>)>> {
    let mut grouped: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, task) in tasks.iter().enumerate() {
        if !task.owns_procurement_task(actor.id()) {
            return Err(Error::ConflictError("供给分配任务缺少冻结责任范围".into()));
        }
        if let Some((_, indexes)) = grouped.iter_mut().find(|(id, _)| *id == task.business_object_id) {
            indexes.push(index);
        } else {
            grouped.push((task.business_object_id.clone(), vec![index]));
        }
    }
    Ok(grouped)
}

/// 每张销售单读取一次覆盖，只为任务范围内尚待分配的 SKU 加载供给。
async fn load_coverages(
    db: &Database,
    orders: &HashMap<String, SalesOrder>,
    tasks: &[WorkItem],
    indexes: &[(String, Vec<usize>)],
) -> Result<(HashMap<String, SalesProcurementCoverage>, Vec<SkuId>)> {
    let mut coverages = HashMap::new();
    let mut sku_ids = HashSet::new();
    for (id, task_indexes) in indexes {
        let Some(order) = orders.get(id) else { continue };
        let coverage = load_sales_procurement_coverage(db, order, &mut NoTransaction).await?;
        let scope = task_indexes
            .iter()
            .flat_map(|&index| tasks[index].responsibility_scope_ids().iter().map(String::as_str))
            .collect::<HashSet<_>>();
        for line in &coverage.lines {
            if scope.contains(line.revision_line.sales_order_line_id.as_ref())
                && line.summary.remaining_quantity > zero_quantity()
            {
                sku_ids.insert(line.goods_line.sku_id.clone());
            }
        }
        coverages.insert(id.clone(), coverage);
    }
    Ok((coverages, sku_ids.into_iter().collect()))
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
    value.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
}
