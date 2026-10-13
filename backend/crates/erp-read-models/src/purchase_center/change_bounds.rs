//! 采购改单逐行数量范围的事实装配（第一期 §6.3“实物与服务采购改单执行规则”）。
//!
//! 已发生数量 = 跨版本累计合格入库 + 本采购供应商直发已发货 + 已确认且未失败的
//! 电子交付和服务履约。可填上限由销售当前覆盖与本采购受影响原因确定。

use std::collections::HashMap;

use erp_core::ids::{PurchaseOrderId, PurchaseOrderRevisionId, SalesOrderId, SalesOrderLineId};
use erp_core::money::Quantity;
use erp_fulfillment::entity::fulfillment::electronic_delivery::FulfillmentResult;
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::entity::purchase_order::{
    ChangeQuantityBound, PurchaseLineType, PurchaseOrder, PurchaseOrderRevisionLine,
    build_change_quantity_bounds, rollup_by_stable_key,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;
use rust_decimal::Decimal;

use super::source_impact::{PurchaseSourceState, load_purchase_source_state};
use crate::{Error, Result};

/// 采购改单所需的销售覆盖状态与逐行数量范围。
pub struct ChangeQuantityFacts {
    /// 销售当前覆盖与受影响状态。
    pub state: PurchaseSourceState,
    /// 采购当前生效版本全部行。
    pub base_lines: Vec<PurchaseOrderRevisionLine>,
    /// 采购当前生效版本商品行的数量范围。
    pub bounds: Vec<ChangeQuantityBound>,
}

/// 读取采购当前生效版本商品行的改单数量范围。
///
/// # 参数
/// * `db` - 数据库
/// * `order` - 已证明可见的已生效采购单
/// * `executor` - 调用方执行器；生效事务内必须传入同一会话
///
/// # 返回
/// 返回销售覆盖状态与按采购行号排列的数量范围。
///
/// # 错误
/// 来源销售或当前版本缺失、覆盖事实不一致或仓储失败时返回错误。
pub async fn load_change_quantity_facts(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<ChangeQuantityFacts> {
    let sales = db
        .sales_orders()
        .find_by_id(order.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
    let state = load_purchase_source_state(db, &sales, executor).await?;
    let current = order
        .stable
        .current_revision_id
        .as_deref()
        .ok_or_else(|| Error::BusinessLogicError("采购单尚未生效，不能改单".into()))?;
    let base_lines =
        db.purchase_order().list_revision_lines(&PurchaseOrderRevisionId::new(current), executor).await?;
    let fulfilled = load_purchase_fulfilled_quantities(db, order, executor).await?;
    let bounds = build_change_quantity_bounds(
        &base_lines,
        &state.coverage,
        state.impact_of(&order.base.id),
        &fulfilled,
    )
    .map_err(Error::Logic)?;
    Ok(ChangeQuantityFacts { state, base_lines, bounds })
}

/// 汇总本采购在各稳定销售行已发生的入库、发货与交付数量。
///
/// # 参数
/// * `db` - 数据库
/// * `order` - 采购单
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回稳定销售行到已发生数量；没有事实的行不出现。
///
/// # 错误
/// 仓储读取失败或累计数量越界时返回错误。
pub async fn load_purchase_fulfilled_quantities(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<HashMap<String, Quantity>> {
    let order_id = PurchaseOrderId::new(&order.base.id);
    let all_lines = all_revision_lines(db, &order_id, executor).await?;
    let received =
        db.fulfillment().qualified_received_totals_by_purchase_revision_line(&order_id, executor).await?;
    let mut rows =
        rollup_by_stable_key(&all_lines, &received).map_err(Error::Logic)?.into_iter().collect::<Vec<_>>();
    let stable_ids = stable_line_ids(&all_lines);
    rows.extend(shipped_rows(db, order, executor).await?);
    rows.extend(
        db.fulfillment()
            .list_confirmed_electronic_deliveries(&stable_ids, executor)
            .await?
            .into_iter()
            .filter(|row| row.purchase_order_id == order_id && row.result != FulfillmentResult::Failure)
            .map(|row| (row.sales_order_line_id.to_string(), row.quantity)),
    );
    rows.extend(
        db.fulfillment()
            .list_confirmed_service_fulfillments(&stable_ids, executor)
            .await?
            .into_iter()
            .filter(|row| row.purchase_order_id == order_id && row.result != FulfillmentResult::Failure)
            .map(|row| (row.sales_order_line_id.to_string(), row.quantity)),
    );
    sum_by_line(rows)
}

/// 本采购全部版本的版本行。
async fn all_revision_lines(
    db: &Database,
    order_id: &PurchaseOrderId,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseOrderRevisionLine>> {
    let revisions = db.purchase_order().list_revisions_by_order(order_id, executor).await?;
    let ids = revisions.into_iter().map(|row| PurchaseOrderRevisionId::new(row.base.id)).collect::<Vec<_>>();
    Ok(db.purchase_order_revision_lines().find_lines_by_revision_ids(&ids, executor).await?)
}

/// 本采购供应商直发已发货或已签收的发货行。
async fn shipped_rows(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<Vec<(String, Quantity)>> {
    let deliveries = db
        .fulfillment()
        .list_acceptance_eligible_deliveries(&SalesOrderId::new(order.sales_order_id.as_ref()), executor)
        .await?
        .into_iter()
        .filter(|row| row.purchase_order_id.as_ref().map(AsRef::as_ref) == Some(order.base.id.as_str()))
        .map(|row| row.base.id.into())
        .collect::<Vec<_>>();
    if deliveries.is_empty() {
        return Ok(Vec::new());
    }
    Ok(db
        .fulfillment()
        .delivery_lines_by_delivery_ids(&deliveries, executor)
        .await?
        .into_iter()
        .map(|line| (line.sales_order_line_id.to_string(), line.quantity))
        .collect())
}

/// 采购各版本商品行引用的稳定销售行。
fn stable_line_ids(lines: &[PurchaseOrderRevisionLine]) -> Vec<SalesOrderLineId> {
    let mut ids = lines
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .filter_map(|line| line.sales_order_line_id.clone())
        .collect::<Vec<_>>();
    ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    ids.dedup();
    ids
}

/// 按稳定销售行累加数量。
fn sum_by_line(rows: impl IntoIterator<Item = (String, Quantity)>) -> Result<HashMap<String, Quantity>> {
    let mut totals: HashMap<String, Decimal> = HashMap::new();
    for (line, value) in rows {
        let total = totals.entry(line).or_default();
        *total = total
            .checked_add(value.to_decimal())
            .ok_or_else(|| Error::BusinessLogicError("累计数量超出范围".into()))?;
    }
    totals
        .into_iter()
        .map(|(line, value)| Ok((line, Quantity::try_from(value).map_err(Error::Logic)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    /// 入库、发货与交付按稳定销售行合并累计。
    #[test]
    fn rows_are_summed_per_stable_line() {
        let totals = sum_by_line([
            ("sol-a".to_string(), qty("2")),
            ("sol-a".to_string(), qty("1.5")),
            ("sol-b".to_string(), qty("4")),
        ])
        .unwrap();

        assert_eq!(totals.get("sol-a"), Some(&qty("3.5")));
        assert_eq!(totals.get("sol-b"), Some(&qty("4")));
        assert!(sum_by_line(Vec::new()).unwrap().is_empty());
    }
}
