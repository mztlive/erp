use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_core::ids::{SalesOrderId, SkuId};
use erp_permissions::purchase_order;
use erp_procurement::entity::purchase_order::{CreationBasisFacts, SalesProcurementCoverage};
use erp_procurement::service::purchase_order::creation_basis::zero_quantity;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::PurchaseOrderReadService;
use super::super::dto::{CreationBasisListParams, CreationBasisView};
use super::super::repository::creation_tasks::{CreationTaskFilter, read_creation_tasks};
use super::super::repository::{
    InquiryBasis, WarehouseRegionFact, active_warehouse_regions, load_creation_basis_facts,
    load_sales_procurement_coverage, sales_order_basis_fact, stock_basis_groups_for_order,
};
use super::mapping::{build_basis_view, build_stock_basis_view};
use crate::sales_center::prices::revision_line_prices;
use crate::{Error, Result};
impl PurchaseOrderReadService {
    /// 读取当前账号开放采购任务范围内仍有剩余量的精确采购创建依据。
    ///
    /// # 参数
    /// * `params` - 可选销售单与供给分配任务筛选
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
        let filter = CreationTaskFilter {
            sales_order_id: normalized_optional_filter(params.sales_order_id.as_deref()),
            work_item_id: normalized_optional_filter(params.work_item_id.as_deref()),
        };
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.creation_basis_views(&actor, filter, executor).await })
            })
            .await
    }

    // 读取采购创建依据时，在单个授权阶段内完成任务、业务事实和价格读取后再交付。
    async fn creation_basis_views(
        &self,
        actor: &AuditActor,
        filter: CreationTaskFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CreationBasisView>> {
        let access = self.rbac.query(actor, &purchase_order::CREATE, executor).await?;
        let tasks = read_creation_tasks(&self.db, &filter, &access, executor).await?;

        let mut views = if tasks.is_empty() {
            Vec::new()
        } else {
            BasisList::load(self, tasks, executor).await?.views(&self.db, executor).await?
        };
        self.fill_basis_prices(&mut views, executor).await?;

        self.rbac.validate(&access, executor).await?;
        Ok(views)
    }

    // 采购创建依据的可见行确定后，只为这些授权任务产出的销售行读取价格，避免扩大展示范围。
    async fn fill_basis_prices(
        &self,
        views: &mut [CreationBasisView],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if views.is_empty() {
            return Ok(());
        }

        let ids = views
            .iter()
            .flat_map(|view| view.lines.iter().map(|line| line.sales_order_revision_line_id.clone()))
            .collect::<Vec<_>>();
        let prices = revision_line_prices(&self.db, &ids, executor).await?;

        for line in views.iter_mut().flat_map(|view| &mut view.lines) {
            line.sales_prices = prices.get(&line.sales_order_revision_line_id).cloned();
        }
        Ok(())
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
    // 依据已授权任务批量装载销售、覆盖及供给，不按行重复查询供给。
    async fn load(
        service: &PurchaseOrderReadService,
        tasks: Vec<WorkItem>,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        let task_indexes = group_tasks(&tasks)?;
        let ids = task_indexes.iter().map(|(id, _)| SalesOrderId::new(id.clone())).collect::<Vec<_>>();
        let orders = service.db.sales_orders().find_effective_orders_by_ids(&ids, executor).await?;
        let owner_names = service
            .resolve_account_names(
                &orders.iter().map(|order| order.stable.created_by.clone()).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let orders = orders.into_iter().map(|order| (order.base.id.clone(), order)).collect();

        let (coverages, sku_ids) =
            load_coverages(&service.db, &orders, &tasks, &task_indexes, executor).await?;
        let facts = load_creation_basis_facts(&service.db, &sku_ids, executor).await?;
        let warehouses = active_warehouse_regions(&service.db, executor).await?;
        Ok(Self { tasks, orders, owner_names, task_indexes, coverages, facts, warehouses })
    }

    // 处理采购创建依据时，每张销售单加载一次询价来源，交由全部所属任务复用。
    async fn views(mut self, db: &Database, executor: &mut dyn Executor) -> Result<Vec<CreationBasisView>> {
        let mut views = Vec::new();
        for (order_id, indexes) in &self.task_indexes {
            let Some(order) = self.orders.get(order_id) else { continue };
            let coverage = self
                .coverages
                .get(order_id)
                .ok_or_else(|| Error::ConflictError("销售覆盖资料已变化，请重新加载".into()))?;
            let inquiry = InquiryBasis::load(db, order, executor).await?;
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

                let stock_groups =
                    stock_basis_groups_for_order(db, order, task.responsibility_scope_ids(), executor)
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

// 按来源销售单归集创建采购的已授权任务前，核对任务仍开放且冻结责任范围有效；维持输入任务顺序。
fn group_tasks(tasks: &[WorkItem]) -> Result<Vec<(String, Vec<usize>)>> {
    let mut grouped: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, task) in tasks.iter().enumerate() {
        if !task.is_open_procurement_task() {
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

// 读取采购创建依据时，每张销售单读取一次覆盖，只为任务范围内尚待分配的 SKU 加载供给。
async fn load_coverages(
    db: &Database,
    orders: &HashMap<String, SalesOrder>,
    tasks: &[WorkItem],
    indexes: &[(String, Vec<usize>)],
    executor: &mut dyn Executor,
) -> Result<(HashMap<String, SalesProcurementCoverage>, Vec<SkuId>)> {
    let mut coverages = HashMap::new();
    let mut sku_ids = HashSet::new();
    for (id, task_indexes) in indexes {
        let Some(order) = orders.get(id) else { continue };
        let coverage = load_sales_procurement_coverage(db, order, executor).await?;
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

// 应用采购创建依据筛选时，将可选查询文本规范化为空或去除首尾空白后的值。
//
// 调用参数：
// * `value` - 可选原始查询文本
//
// 空白值返回 `None`，否则返回规范化字符串。
fn normalized_optional_filter(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
}
