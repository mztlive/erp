//! 发送前在同一事务重读原实际收货证据并写入原事实版本屏障。

use std::collections::BTreeSet;

use erp_core::ids::{DeliveryId, PurchaseOrderId, PurchaseOrderRevisionLineId, PurchaseReceiptId};
use erp_fulfillment::entity::fulfillment::FulfillmentFactType;
use erp_fulfillment::repository::{FulfillmentExt, receipt_confirmation};
use erp_procurement::entity::purchase_order::{FulfillmentResponsibility, PurchaseOrder};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_returns::repository::supplier_receipt::has_actual_return;
use erp_supply::api_order::{ApiOrderExecution, for_purchase};
use erp_supply::order_tracking::OrderTracking;
use mongodb::Database;
use persistence_core::Executor;

use super::evidence::{ActualReceiptFacts, DirectReceiptFacts, WarehouseReceiptFacts};
use crate::{Error, Result};

pub(super) struct LoadedReceiptFacts {
    pub purchase: PurchaseOrder,
    pub actual: ActualReceiptFacts,
}

pub(super) async fn load(
    db: &Database,
    execution: &ApiOrderExecution,
    tracking: &OrderTracking,
    executor: &mut dyn Executor,
) -> Result<LoadedReceiptFacts> {
    let purchase = db
        .purchase_orders()
        .find_by_id(&execution.purchase_order_id, executor)
        .await?
        .filter(|purchase| purchase.supplier_id.as_ref() == execution.supplier_id)
        .ok_or_else(|| Error::BusinessLogicError("实际收货缺少原采购或供应商归属不一致".into()))?;
    if purchase.stable.current_revision_id.as_deref() != Some(execution.purchase_order_revision_id.as_str())
        || execution.superseded_by_purchase_revision_id.is_some()
        || tracking.base.id != execution.base.id
        || tracking.connection_id != execution.connection_id
        || tracking.purchase_order_id != execution.purchase_order_id
    {
        return Err(Error::BusinessLogicError("实际收货的采购修订或原执行关联已经变化，请核对原单".into()));
    }
    if has_actual_return(db, &execution.purchase_order_id, executor).await? {
        return Err(Error::BusinessLogicError(
            "原采购已有实际退货，请先核对原退货与供应商子单，未用原入库或验收量推定全部收到".into(),
        ));
    }
    let actual = match purchase.fulfillment_responsibility {
        FulfillmentResponsibility::SupplierDirect => {
            ActualReceiptFacts::Direct(direct(db, execution, tracking, executor).await?)
        },
        FulfillmentResponsibility::Warehouse => {
            ActualReceiptFacts::Warehouse(warehouse(db, execution, &purchase, executor).await?)
        },
        _ => {
            return Err(Error::BusinessLogicError(
                "当前采购不属于实物直发或实际入仓，不能通知供应商收货".into(),
            ));
        },
    };
    Ok(LoadedReceiptFacts { purchase, actual })
}

async fn direct(
    db: &Database,
    execution: &ApiOrderExecution,
    tracking: &OrderTracking,
    executor: &mut dyn Executor,
) -> Result<DirectReceiptFacts> {
    let ids = tracking
        .source_lines
        .iter()
        .filter_map(|line| line.delivery_id.clone())
        .chain(tracking.applied_shipments.iter().flat_map(|shipment| shipment.delivery_ids.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let deliveries = receipt_confirmation::deliveries(db, &ids, executor).await?;
    let delivery_ids = deliveries.iter().map(|row| DeliveryId::new(&row.base.id)).collect::<Vec<_>>();
    let delivery_lines = db.fulfillment().delivery_lines_by_delivery_ids(&delivery_ids, executor).await?;
    let line_ids = delivery_lines.iter().map(|row| row.base.id.clone()).collect::<Vec<_>>();
    let allocations = db
        .fulfillment()
        .allocations_by_fulfillment_fact(FulfillmentFactType::Delivery, &line_ids, executor)
        .await?;
    let ids = allocations
        .iter()
        .map(|row| row.customer_acceptance_line_id.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let acceptance_lines = receipt_confirmation::acceptance_lines(db, &ids, executor).await?;
    let ids = acceptance_lines
        .iter()
        .map(|row| row.customer_acceptance_id.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let acceptances = receipt_confirmation::acceptances(db, &ids, executor).await?;
    let ids = execution
        .source_lines
        .iter()
        .map(|row| PurchaseOrderRevisionLineId::new(&row.purchase_order_revision_line_id))
        .collect::<Vec<_>>();
    let purchase_allocations =
        db.purchase_line_sales_allocations().find_by_purchase_revision_line_ids(&ids, executor).await?;
    Ok(DirectReceiptFacts {
        deliveries,
        delivery_lines,
        allocations,
        acceptance_lines,
        acceptances,
        purchase_allocations,
    })
}

async fn warehouse(
    db: &Database,
    execution: &ApiOrderExecution,
    purchase: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<WarehouseReceiptFacts> {
    let warehouse_id = purchase
        .target_warehouse_id
        .as_ref()
        .map(ToString::to_string)
        .ok_or_else(|| Error::BusinessLogicError("原采购缺少明确收货仓库".into()))?;
    let receipts = receipt_confirmation::purchase_receipts(
        db,
        &PurchaseOrderId::new(&execution.purchase_order_id),
        executor,
    )
    .await?;
    let ids = receipts.iter().map(|row| PurchaseReceiptId::new(&row.base.id)).collect::<Vec<_>>();
    let lines = db.fulfillment().receipt_lines_by_receipt_ids(&ids, executor).await?;
    let purchase_lines = db
        .purchase_order()
        .list_revision_lines(&execution.purchase_order_revision_id.clone().into(), executor)
        .await?;
    let executions =
        for_purchase(db, &execution.purchase_order_id, Some(&execution.purchase_order_revision_id), executor)
            .await?;
    if executions.len() > 1000 {
        return Err(Error::BusinessLogicError("原采购的执行数量超过本次核验范围，请分批核对".into()));
    }
    Ok(WarehouseReceiptFacts { receipts, lines, purchase_lines, executions, warehouse_id })
}

impl LoadedReceiptFacts {
    pub(super) async fn fence(&mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        db.purchase_orders().update(&mut self.purchase, executor).await?;
        match &mut self.actual {
            ActualReceiptFacts::Direct(facts) => {
                for row in &mut facts.deliveries {
                    db.deliveries().update(row, executor).await?;
                }
                for row in &mut facts.delivery_lines {
                    db.delivery_lines().update(row, executor).await?;
                }
                for row in &mut facts.acceptances {
                    db.customer_acceptances().update(row, executor).await?;
                }
                for row in &mut facts.acceptance_lines {
                    db.customer_acceptance_lines().update(row, executor).await?;
                }
            },
            ActualReceiptFacts::Warehouse(facts) => {
                for row in &mut facts.receipts {
                    db.purchase_receipts().update(row, executor).await?;
                }
                for row in &mut facts.lines {
                    db.purchase_receipt_lines().update(row, executor).await?;
                }
            },
        }
        Ok(())
    }
}
