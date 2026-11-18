use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesReviewExt;
use erp_workflow::entity::work_item::WorkItemLink;
use mongodb::Database;
use persistence_core::Executor;

use super::{link, required};
use crate::Result;
// 处理订单任务上级链路时，读取当前写事务确认的直接上级引用。
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
    let parent = match kind {
        "purchase_order" => link(
            "sales_order",
            required(db.purchase_orders().find_by_id(id, executor).await?)?.sales_order_id,
        )?,
        "sales_change_order" => link(
            "sales_order",
            required(db.sales_change_orders().find_by_id(id, executor).await?)?.sales_order_id,
        )?,
        "purchase_change_order" => link(
            "purchase_order",
            required(db.purchase_change_orders().find_by_id(id, executor).await?)?.purchase_order_id,
        )?,
        _ => unreachable!("订单链路分派已限定类型"),
    };
    Ok(vec![parent])
}
