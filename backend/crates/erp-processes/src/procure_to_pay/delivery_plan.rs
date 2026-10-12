//! 采购变更在冻结和生效时复验地址，沿用销售 guard 与调用方执行器。
use erp_core::ids::SalesOrderRevisionId;
use erp_core::money::Quantity;
use erp_procurement::entity::purchase_order::{
    ProcurementCoverageFacts, PurchaseDeliveryAllocation, PurchaseLineInput, PurchaseOrder,
};
use erp_read_models::purchase_center::repository::load_procurement_coverage_facts;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

pub(super) type DeliveryLine = (String, Quantity, Vec<PurchaseDeliveryAllocation>);

/// 用销售当前版本和其他有效采购安排复验本单交付。
/// # 参数
/// `db`、`executor` 沿用调用方读取边界；`order` 为采购单，`lines` 为目标行。
/// # 返回
/// 当前地址、单位精度和地址剩余量均合法时成功。
/// # 错误
/// 来源失效、地址被修改或其他采购已占用目标数量时拒绝。
pub(super) async fn validate_delivery(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[DeliveryLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    if lines.iter().all(|(_, _, rows)| rows.is_empty()) {
        return Ok(());
    }
    let sales = db
        .sales_orders()
        .find_by_id(order.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
    let revision_id =
        sales.stable.current_revision_id.ok_or_else(|| Error::ConflictError("销售单缺少生效版本".into()))?;
    let facts = load_procurement_coverage_facts(
        db,
        &SalesOrderRevisionId::new(revision_id),
        &order.sales_order_id,
        executor,
    )
    .await?;
    validate_lines(order, lines, &facts)
}

fn validate_lines(
    order: &PurchaseOrder,
    lines: &[DeliveryLine],
    facts: &ProcurementCoverageFacts,
) -> Result<()> {
    for (stable, quantity, rows) in lines.iter().filter(|(_, _, rows)| !rows.is_empty()) {
        let source = facts
            .revision_lines
            .iter()
            .find(|line| line.sales_order_line_id.as_ref() == stable)
            .ok_or_else(|| Error::ConflictError("销售商品行已变化，请重新核对交付安排".into()))?;
        let goods = facts
            .goods_lines
            .iter()
            .find(|line| line.revision_line_id.as_ref() == source.base.id)
            .ok_or_else(|| Error::ConflictError("销售交付要求暂不可用".into()))?;
        let assigned = other_deliveries(order, facts, stable);
        PurchaseDeliveryAllocation::validate_source(
            rows,
            &goods.delivery_allocations,
            &assigned,
            *quantity,
            order.fulfillment_responsibility,
            facts.quantity_scales.get(goods.sku_id.as_ref()).copied(),
            &goods.base_unit_code,
        )?;
    }
    Ok(())
}

fn other_deliveries(
    order: &PurchaseOrder,
    facts: &ProcurementCoverageFacts,
    stable: &str,
) -> Vec<PurchaseDeliveryAllocation> {
    facts
        .submission_lines
        .iter()
        .filter(|line| {
            line.sales_order_line_id.as_ref().is_some_and(|id| id.as_ref() == stable)
                && order.current_submission_id.as_deref() != Some(line.purchase_order_submission_id.as_ref())
        })
        .flat_map(|line| line.delivery_allocations.clone())
        .chain(
            facts
                .purchase_revision_lines
                .iter()
                .filter(|line| {
                    line.sales_order_line_id.as_ref().is_some_and(|id| id.as_ref() == stable)
                        && order.stable.current_revision_id.as_deref()
                            != Some(line.purchase_order_revision_id.as_ref())
                })
                .flat_map(|line| line.delivery_allocations.clone()),
        )
        .collect::<Vec<_>>()
}

/// 采购变更输入转为待复验交付行。
/// # 参数
/// `db`、`executor` 为调用方执行器；`order`、`inputs` 为本次变更内容。
/// # 返回
/// 完整通过交付来源校验时成功。
/// # 错误
/// 地址、数量、来源或数据读取失败时拒绝。
pub(super) async fn validate_input_delivery(
    db: &Database,
    order: &PurchaseOrder,
    inputs: &[PurchaseLineInput],
    executor: &mut dyn Executor,
) -> Result<()> {
    let lines = inputs
        .iter()
        .filter_map(|line| {
            Some((
                line.sales_order_line_id.as_ref()?.to_string(),
                line.quantity?,
                line.delivery_allocations.clone(),
            ))
        })
        .collect::<Vec<_>>();
    validate_delivery(db, order, &lines, executor).await
}
