use erp_fulfillment::repository::FulfillmentExt;
use erp_workflow::entity::work_item::WorkItemLink;
use mongodb::Database;
use persistence_core::Executor;

use super::{link, required};
use crate::Result;
// 处理履约任务上级链路时，读取当前写事务确认的直接上级引用。
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
        "purchase_receipt" => Ok(vec![link(
            "purchase_order",
            required(db.purchase_receipts().find_by_id(id, executor).await?)?.purchase_order_id,
        )?]),
        "electronic_delivery" => Ok(vec![link(
            "purchase_order",
            required(db.electronic_deliveries().find_by_id(id, executor).await?)?.purchase_order_id,
        )?]),
        "service_fulfillment" => Ok(vec![link(
            "purchase_order",
            required(db.service_fulfillments().find_by_id(id, executor).await?)?.purchase_order_id,
        )?]),
        "customer_acceptance" => Ok(vec![link(
            "sales_order",
            required(db.customer_acceptances().find_by_id(id, executor).await?)?.sales_order_id,
        )?]),
        "delivery" => {
            let row = required(db.deliveries().find_by_id(id, executor).await?)?;
            let mut links = vec![link("sales_order", row.sales_order_id)?];
            if let Some(id) = row.purchase_order_id {
                links.push(link("purchase_order", id)?);
            }
            Ok(links)
        },
        _ => unreachable!("履约链路分派已限定类型"),
    }
}
