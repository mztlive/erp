//! 电子交付和线下服务确认后的实际采购成本；与履约事实复用同一个事务。

use std::str::FromStr;

use erp_core::common::time::Instant;
use erp_core::ids::{FileAssetId, PurchaseLineSalesAllocationId, SalesOrderLineId};
use erp_core::money::{Quantity, Rate};
use erp_finance::service::cost::fulfillment_actual::{FulfillmentCostFact, prepare};
use erp_finance::service::cost::persist_cost_entry;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::repository::PurchaseOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 已确认履约的金额来源和业务凭证，类型间共用同一采购分配唯一身份。
pub(super) struct ActualCostSource<'a> {
    pub purchase: &'a PurchaseOrder,
    pub allocation_id: &'a PurchaseLineSalesAllocationId,
    pub sales_order_line_id: &'a SalesOrderLineId,
    pub quantity: Quantity,
    pub occurred_at: Instant,
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 将已确认的整笔采购销售分配记入实际成本，不另开事务。
///
/// # 参数
/// * `db` - 业务数据库
/// * `source` - 已通过采购资格、当前分配和履约状态校验的来源
/// * `executor` - 包含履约确认的根事务执行器
///
/// # 返回
/// 成本及精确销售明细分配写入成功时返回 `Ok(())`。
///
/// # 错误
/// 来源缺失、履约数量超出冻结分配、成本非法或重复过账时回滚整个确认事务。
pub(super) async fn post(
    db: &Database,
    source: ActualCostSource<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let allocation = db
        .purchase_line_sales_allocations()
        .find_by_id(source.allocation_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("履约成本的采购销售分配不存在".into()))?;
    let line = db
        .purchase_order_revision_lines()
        .find_by_id(allocation.purchase_order_revision_line_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("履约成本的采购版本明细不存在".into()))?;
    let prepared = prepare(FulfillmentCostFact {
        purchase_order_id: source.purchase.base.id.clone(),
        allocation_id: allocation.base.id.clone(),
        allocation_version: allocation.base.version,
        allocated_quantity: allocation.allocated_quantity,
        fulfilled_quantity: source.quantity,
        gross_amount: allocation.allocated_cost_gross,
        net_amount: allocation.allocated_cost_net,
        input_tax_rate: line.input_tax_rate.unwrap_or(Rate::from_str("0")?),
        supplier_id: source.purchase.supplier_id.clone(),
        sales_order_id: source.purchase.sales_order_id.clone(),
        sales_order_line_id: source.sales_order_line_id.clone(),
        occurred_at: source.occurred_at,
        evidence_attachment_id: source.evidence_attachment_id,
    })?;
    if let Some(prepared) = prepared {
        persist_cost_entry(db, prepared, executor).await?;
    }
    Ok(())
}
