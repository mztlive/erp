//! 仅供写流程冻结任务链路；任务列表、详情和统计禁止调用本模块。
mod finance;
mod fulfillment;
mod orders;
mod returns;
use std::collections::BTreeSet;

use erp_workflow::entity::work_item::WorkItemLink;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 创建任务或切换业务版本时，在原写入事务捕获直接对象和真实上级引用；任务列表、详情及统计读取不得调用此入口。
///
/// # 参数
/// `db`、`executor` 为原业务事务，`kind`、`id` 为该版本已持久化的直接对象。
///
/// # 返回
/// 返回包含直接对象的类型与 ID 配对集合，不包含姓名或业务材料。
///
/// # 错误
/// 上级记录缺失、链路超过 1000 个或仓储读取失败时拒绝。
pub async fn capture(
    db: &Database,
    kind: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItemLink>> {
    let mut visited = BTreeSet::new();
    let mut pending = vec![link(kind, id)?];
    while let Some(current) = pending.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        if visited.len() > 1000 {
            return Err(Error::ValidationError("任务关联对象最多1000个".into()));
        }
        pending.extend(parents(db, current.object_type(), current.object_id(), executor).await?);
    }
    Ok(visited.into_iter().collect())
}

async fn parents(
    db: &Database,
    kind: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItemLink>> {
    match kind {
        "voucher_sales_order" => Ok(vec![link("sales_order", id)?]),
        "purchase_order" | "sales_change_order" | "purchase_change_order" => {
            orders::parents(db, kind, id, executor).await
        },
        "purchase_receipt"
        | "delivery"
        | "electronic_delivery"
        | "service_fulfillment"
        | "customer_acceptance" => fulfillment::parents(db, kind, id, executor).await,
        "receivable_account"
        | "payable_account"
        | "receivable_entry"
        | "payable_entry"
        | "sales_invoice_request"
        | "purchase_payment_request"
        | "customer_receipt"
        | "supplier_payment"
        | "invoice" => finance::parents(db, kind, id, executor).await,
        "sales_return_case"
        | "purchase_return_order"
        | "customer_refund"
        | "supplier_refund"
        | "receipt_reversal"
        | "payment_reversal" => returns::parents(db, kind, id, executor).await,
        _ => Ok(Vec::new()),
    }
}
fn link(kind: &str, id: impl ToString) -> Result<WorkItemLink> {
    Ok(WorkItemLink::new(kind, id.to_string())?)
}
fn required<T>(value: Option<T>) -> Result<T> {
    value.ok_or_else(|| Error::ConflictError("任务链路引用的业务对象不存在或已删除".into()))
}
