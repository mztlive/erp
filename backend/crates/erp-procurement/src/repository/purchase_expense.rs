//! 采购费用行确认发生事实的拥有仓储；唯一索引保证每个费用行只确认一次。
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::purchase_order::{PurchaseExpenseOccurrence, PurchaseOrder, PurchaseOrderStatus};
use crate::repository::PurchaseOrderExt;

/// 费用行确认发生事实集合名。
pub const PURCHASE_EXPENSE_OCCURRENCES: &str = "purchase_expense_occurrences";

/// 获取费用行确认发生仓储。
///
/// # 参数
/// * `db` - 业务数据库
///
/// # 返回
/// 返回使用调用方 Executor 的仓储。
///
/// # 错误
/// 无。
pub fn purchase_expense_occurrences(db: &Database) -> Repository<'_, PurchaseExpenseOccurrence> {
    Repository::new(db, PURCHASE_EXPENSE_OCCURRENCES)
}

/// 读取一组采购单的全部费用确认发生事实。
///
/// # 参数
/// * `db` - 业务数据库
/// * `purchase_order_ids` - 采购单主键；空集合直接返回空
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回这些采购单已确认发生的费用行事实。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn occurrences_by_orders(
    db: &Database,
    purchase_order_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseExpenseOccurrence>> {
    if purchase_order_ids.is_empty() {
        return Ok(Vec::new());
    }
    purchase_expense_occurrences(db)
        .find_many(doc! { "purchase_order_id": { "$in": purchase_order_ids } }, executor)
        .await
}

/// 批量读取一组销售单下已生效（含部分执行、已完成）的采购单，供附加费用汇总。
///
/// # 参数
/// * `db` - 业务数据库
/// * `sales_order_ids` - 销售单主键；空集合直接返回空
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回未删除且已形成生效版本的采购单。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn formal_orders_by_sales_orders(
    db: &Database,
    sales_order_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseOrder>> {
    if sales_order_ids.is_empty() {
        return Ok(Vec::new());
    }
    let filter = doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "sales_order_id": { "$in": sales_order_ids },
        "status": { "$in": [
            PurchaseOrderStatus::Effective.as_str(),
            PurchaseOrderStatus::PartiallyExecuted.as_str(),
            PurchaseOrderStatus::Completed.as_str(),
        ]},
    };
    db.purchase_orders().find_many(filter, executor).await
}

/// 读取一条费用行的确认发生事实。
///
/// # 参数
/// * `db` - 业务数据库
/// * `purchase_order_id` - 采购单
/// * `expense_line_id` - 稳定费用行身份
/// * `executor` - 调用方执行器
///
/// # 返回
/// 已确认时返回事实，否则返回 `None`。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn find_occurrence(
    db: &Database,
    purchase_order_id: &str,
    expense_line_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<PurchaseExpenseOccurrence>> {
    purchase_expense_occurrences(db)
        .find_one(
            doc! { "purchase_order_id": purchase_order_id, "expense_line_id": expense_line_id },
            executor,
        )
        .await
}
