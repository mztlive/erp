//! 销售单附加费用列表：财务登记费用与采购费用行（§7.5.1）。
//!
//! 在同一事务证明成本列表资格和销售单详情范围，读取全部费用后完成交付复验。
//! 附加费用只按销售单归属读取，不按供应商成本推导。
//! 采购费用行取每张非作废采购单的当前生效版本：已确认发生的按确认事实展示，
//! 其余金额为正的费用行为待发生。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::PurchaseOrderRevisionId;
use erp_core::money::Amount;
use erp_finance::dto::extra_cost::{
    ExtraCostSource, ExtraCostStatus, SalesExtraCostListView, SalesExtraCostView,
};
use erp_finance::entity::cost::{CostAllocation, CostEntry, ExtraCostCategory, business_day_start};
use erp_finance::repository::CostExt;
use erp_finance::repository::cost::{CostAllocationProfitLossExt, CostEntryProfitLossExt};
use erp_identity::AuthorizationService;
use erp_permissions::internal;
use erp_procurement::entity::purchase_order::{
    PurchaseExpenseCategory, PurchaseExpenseOccurrence, PurchaseLineType, PurchaseOrder,
    PurchaseOrderRevisionLine,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::repository::purchase_expense::{formal_orders_by_sales_orders, occurrences_by_orders};
use erp_sales::repository::sales_order::scope::SalesReadScope;
use erp_sales::repository::{SalesOrderExt, SalesOrderRepositoryScopeExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{Error, Result};

/// 销售单附加费用读取服务。
#[derive(Clone)]
pub struct SalesExtraCostReadModel {
    db: Database,
    rbac: Arc<AuthorizationService>,
}

impl SalesExtraCostReadModel {
    /// 装配销售附加费用时，绑定数据库与当前授权服务。
    ///
    /// # 参数
    /// * `rbac` - 当前 RBAC 服务
    ///
    /// # 返回
    /// 返回读取服务，构造不执行 I/O。
    pub fn new(db: Database, rbac: Arc<AuthorizationService>) -> Self {
        Self { db, rbac }
    }

    /// 读取销售附加费用时，列出销售单上的附加费用。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单
    /// * `actor` - 当前身份；本用例验证成本列表资格和销售详情授权。
    ///
    /// # 返回
    /// 返回按发生时间排序的附加费用及待发生、已发生合计。
    ///
    /// # 错误
    /// 成本资格缺失时拒绝；销售单不存在或不可见时返回 `NotFound`；读取或交付失败原样返回。
    pub async fn list(&self, sales_order_id: &str, actor: &AuditActor) -> Result<SalesExtraCostListView> {
        let this = self.clone();
        let actor = actor.clone();
        let sales_order_id = sales_order_id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.read(&sales_order_id, &actor, executor).await })
            })
            .await
    }

    // 读取销售附加费用时，以本次快照完成固定入口授权、费用读取和最终结果交付检查。
    async fn read(
        &self,
        sales_order_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesExtraCostListView> {
        let access =
            Arc::new(self.rbac.query(actor, &internal::SALES_ORDER_EXTRA_COST_READ, executor).await?);
        let scope = SalesReadScope::from_compiled(access.clone(), self.rbac.clone());
        self.db
            .sales_orders()
            .find_authorized(sales_order_id, &scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在或无权查看附加费用".into()))?;

        let (entries, allocations) = order_costs(&self.db, sales_order_id, executor).await?;
        let mut items = SalesExtraCostView::registrations(&entries, &allocations);
        let facts = purchase_expense_facts(&self.db, &[sales_order_id.to_string()], executor).await?;
        items.extend(purchase_expense_views(&facts));

        self.rbac.validate(&access, executor).await?;
        Ok(SalesExtraCostListView::from_items(items))
    }
}

/// 处理销售附加费用时，读取归属销售单的全部成本事实及分配。
///
/// # 参数
/// * `sales_order_id` - 销售单
///
/// # 返回
/// 返回成本事实及其分配。
///
/// # 错误
/// 仓储读取失败时返回对应错误。
pub async fn order_costs(
    db: &Database,
    sales_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<(Vec<CostEntry>, Vec<CostAllocation>)> {
    let allocations =
        db.cost_allocations().profit_loss_allocations(&[sales_order_id.to_string()], executor).await?;
    let ids = allocations.iter().map(|line| line.cost_entry_id.to_string()).collect::<Vec<_>>();
    let entries = db.cost_entries().profit_loss_entries(&ids, executor).await?;
    Ok((entries, allocations))
}

/// 一组销售单上的采购费用行事实。
#[derive(Debug, Default)]
pub struct PurchaseExpenseFacts {
    /// 非作废且已生效的采购单。
    pub orders: Vec<PurchaseOrder>,
    /// 这些采购单当前版本的费用行。
    pub lines: Vec<PurchaseOrderRevisionLine>,
    /// 已确认发生事实。
    pub occurrences: Vec<PurchaseExpenseOccurrence>,
}

impl PurchaseExpenseFacts {
    /// 处理销售附加费用时，已生效、金额为正且尚未确认发生的费用行所属销售单。
    ///
    /// # 返回
    /// 返回存在待发生附加费用的销售单集合。
    pub fn sales_orders_with_pending(&self) -> HashSet<String> {
        let occurred = self.occurred_keys();
        let order_of = self.revision_orders();
        self.lines
            .iter()
            .filter(|line| line.gross_amount.to_decimal() > rust_decimal::Decimal::ZERO)
            .filter_map(|line| {
                let order = order_of.get(line.purchase_order_revision_id.as_ref())?;
                let expense = line.expense.as_ref()?;
                (!occurred.contains(&(order.base.id.clone(), expense.expense_line_id.clone())))
                    .then(|| order.sales_order_id.to_string())
            })
            .collect()
    }

    fn occurred_keys(&self) -> HashSet<(String, String)> {
        self.occurrences
            .iter()
            .map(|occurrence| (occurrence.purchase_order_id.to_string(), occurrence.expense_line_id.clone()))
            .collect()
    }

    fn revision_orders(&self) -> HashMap<&str, &PurchaseOrder> {
        self.orders
            .iter()
            .filter_map(|order| Some((order.stable.current_revision_id.as_deref()?, order)))
            .collect()
    }
}

/// 处理销售附加费用时，批量读取一组销售单的采购费用行与确认事实。
///
/// # 参数
/// * `sales_order_ids` - 销售单
///
/// # 返回
/// 返回已生效采购单当前版本的费用行及确认发生事实。
///
/// # 错误
/// 仓储读取失败时返回对应错误。
pub async fn purchase_expense_facts(
    db: &Database,
    sales_order_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<PurchaseExpenseFacts> {
    let mut facts = PurchaseExpenseFacts::default();
    for chunk in sales_order_ids.chunks(500) {
        let orders = formal_orders_by_sales_orders(db, chunk, executor).await?;
        let orders =
            orders.into_iter().filter(|order| order.stable.current_revision_id.is_some()).collect::<Vec<_>>();

        let revision_ids = orders
            .iter()
            .filter_map(|order| order.stable.current_revision_id.clone().map(PurchaseOrderRevisionId::new))
            .collect::<Vec<_>>();
        facts.lines.extend(
            db.purchase_order_revision_lines()
                .find_lines_by_revision_ids(&revision_ids, executor)
                .await?
                .into_iter()
                .filter(|line| line.line_type == PurchaseLineType::LogisticsFee && line.expense.is_some()),
        );

        let order_ids = orders.iter().map(|order| order.base.id.clone()).collect::<Vec<_>>();
        facts.occurrences.extend(occurrences_by_orders(db, &order_ids, executor).await?);
        facts.orders.extend(orders);
    }
    Ok(facts)
}

/// 处理销售附加费用时，采购费用行映射为附加费用视图：已确认发生的按确认事实，金额为正的其余行为待发生。
///
/// # 参数
/// * `facts` - 采购费用行事实
///
/// # 返回
/// 返回附加费用视图，未排序。
pub fn purchase_expense_views(facts: &PurchaseExpenseFacts) -> Vec<SalesExtraCostView> {
    let occurred = facts
        .occurrences
        .iter()
        .map(|occurrence| {
            ((occurrence.purchase_order_id.to_string(), occurrence.expense_line_id.clone()), occurrence)
        })
        .collect::<HashMap<_, _>>();
    let order_of = facts.revision_orders();
    let mut views = Vec::new();
    for line in &facts.lines {
        let (Some(order), Some(expense)) =
            (order_of.get(line.purchase_order_revision_id.as_ref()), line.expense.as_ref())
        else {
            continue;
        };
        let key = (order.base.id.clone(), expense.expense_line_id.clone());
        let view = match occurred.get(&key) {
            Some(occurrence) => occurred_view(order, occurrence),
            None if line.gross_amount.to_decimal() > rust_decimal::Decimal::ZERO => pending_view(order, line),
            None => continue,
        };
        views.push(view);
    }
    views
}

fn pending_view(order: &PurchaseOrder, line: &PurchaseOrderRevisionLine) -> SalesExtraCostView {
    let expense = line.expense.as_ref().expect("调用方已筛选费用行");
    SalesExtraCostView {
        source: ExtraCostSource::PurchaseExpense,
        id: expense.expense_line_id.clone(),
        category: extra_cost_category(expense.category),
        description: expense.description.clone(),
        gross_amount: line.gross_amount,
        net_amount: line.net_amount,
        input_tax_rate: line.input_tax_rate.unwrap_or_else(|| "0".parse().expect("零税率合法")),
        status: ExtraCostStatus::Pending,
        occurred_at: None,
        sales_order_line_id: expense.sales_order_line_id.as_ref().map(ToString::to_string),
        supplier_id: Some(order.supplier_id.to_string()),
        purchase_order_id: Some(order.base.id.clone()),
        purchase_no: Some(order.purchase_no.clone()),
        evidence_attachment_id: None,
        reduced_gross_amount: Amount::zero(),
        remaining_gross_amount: line.gross_amount,
        reductions: Vec::new(),
    }
}

fn occurred_view(order: &PurchaseOrder, occurrence: &PurchaseExpenseOccurrence) -> SalesExtraCostView {
    SalesExtraCostView {
        source: ExtraCostSource::PurchaseExpense,
        id: occurrence.expense_line_id.clone(),
        category: extra_cost_category(occurrence.expense.category),
        description: occurrence.expense.description.clone(),
        gross_amount: occurrence.gross_amount,
        net_amount: occurrence.net_amount,
        input_tax_rate: occurrence.input_tax_rate,
        status: ExtraCostStatus::Occurred,
        occurred_at: Some(business_day_start(occurrence.occurred_on)),
        sales_order_line_id: occurrence.expense.sales_order_line_id.as_ref().map(ToString::to_string),
        supplier_id: Some(order.supplier_id.to_string()),
        purchase_order_id: Some(order.base.id.clone()),
        purchase_no: Some(order.purchase_no.clone()),
        evidence_attachment_id: Some(occurrence.evidence_attachment_id.to_string()),
        reduced_gross_amount: Amount::zero(),
        remaining_gross_amount: occurrence.gross_amount,
        reductions: Vec::new(),
    }
}

/// 处理销售附加费用时，采购费用类别映射为成本费用类别（稳定代码一致）。
///
/// # 参数
/// * `category` - 采购费用类别
///
/// # 返回
/// 返回同代码的附加费用类别。
pub fn extra_cost_category(category: PurchaseExpenseCategory) -> ExtraCostCategory {
    match category {
        PurchaseExpenseCategory::Logistics => ExtraCostCategory::Logistics,
        PurchaseExpenseCategory::Printing => ExtraCostCategory::Printing,
        PurchaseExpenseCategory::Storage => ExtraCostCategory::Storage,
        PurchaseExpenseCategory::Delivery => ExtraCostCategory::Delivery,
        PurchaseExpenseCategory::PlatformTech => ExtraCostCategory::PlatformTech,
        PurchaseExpenseCategory::OfflineService => ExtraCostCategory::OfflineService,
        PurchaseExpenseCategory::Other => ExtraCostCategory::Other,
    }
}

#[cfg(test)]
mod tests;
