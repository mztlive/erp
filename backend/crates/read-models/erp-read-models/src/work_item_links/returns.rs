use erp_returns::repository::ReturnsExt;
use erp_workflow::entity::work_item::WorkItemLink;
use mongodb::Database;
use persistence_core::Executor;

use super::{link, required};
use crate::Result;
// 处理退货任务上级链路时，读取当前写事务确认的直接上级引用。
//
// 调用参数：
// 数据库、已分派的对象类型与 ID、调用方执行器。
//
// 返回类型与 ID 成对的上级业务对象。
//
// 对象缺失、引用非法或仓储读取失败时返回错误。
pub(super) async fn parents(
    db: &Database,
    kind: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItemLink>> {
    match kind {
        "sales_return_case" => Ok(vec![link(
            "sales_order",
            required(db.sales_return_cases().find_by_id(id, executor).await?)?.sales_order_id,
        )?]),
        "purchase_return_order" => {
            let row = required(db.purchase_return_orders().find_by_id(id, executor).await?)?;
            let mut links = vec![link("purchase_order", row.purchase_order_id)?];
            if let Some(id) = row.sales_return_case_id {
                links.push(link("sales_return_case", id)?);
            }
            Ok(links)
        },
        "receipt_reversal" => Ok(vec![link(
            "customer_receipt",
            required(db.receipt_reversals().find_by_id(id, executor).await?)?.original_customer_receipt_id,
        )?]),
        "payment_reversal" => Ok(vec![link(
            "supplier_payment",
            required(db.payment_reversals().find_by_id(id, executor).await?)?.original_supplier_payment_id,
        )?]),
        _ => refunds(db, kind, id, executor).await,
    }
}
async fn refunds(
    db: &Database,
    kind: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItemLink>> {
    let mut links = Vec::new();
    if kind == "customer_refund" {
        let row = required(db.customer_refunds().find_by_id(id, executor).await?)?;
        if let Some(id) = row.sales_return_case_id {
            links.push(link("sales_return_case", id)?);
        }
        if let Some(id) = row.original_receipt_id {
            links.push(link("customer_receipt", id)?);
        }
        if let Some(id) = row.original_receivable_entry_id {
            links.push(link("receivable_entry", id)?);
        }
    } else {
        let row = required(db.supplier_refunds().find_by_id(id, executor).await?)?;
        if let Some(id) = row.purchase_return_order_id {
            links.push(link("purchase_return_order", id)?);
        }
        if let Some(id) = row.original_payment_id {
            links.push(link("supplier_payment", id)?);
        }
        if let Some(id) = row.original_payable_entry_id {
            links.push(link("payable_entry", id)?);
        }
        if let Some(id) = row.source_purchase_order_id {
            links.push(link("purchase_order", id)?);
        }
    }
    Ok(links)
}
