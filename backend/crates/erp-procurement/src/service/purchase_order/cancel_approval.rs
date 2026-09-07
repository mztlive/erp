//! 采购撤回审批后的单据状态持久化。

use crate::entity::purchase_order::PurchaseOrder;
use crate::repository::PurchaseOrderExt;
use crate::Result;
use persistence_core::Executor;

/// 在审批取消运行事实写入后，以同一执行器 CAS 写回已执行撤回动作的采购单。
///
/// # 错误
/// 采购单版本冲突或仓储写入失败时返回原错误。
///
/// # 关键业务约束
/// 调用方持有根事务并先执行采购撤回状态动作；本方法不得另开事务。
pub async fn persist_cancelled_order(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.purchase_orders().update(order, executor).await?;
    Ok(())
}
