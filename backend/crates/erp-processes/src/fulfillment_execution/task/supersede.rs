//! 采购变更停止未发 API 执行时，关闭其自动生成草稿的唯一履约任务。

use erp_core::common::time::Instant;
use erp_fulfillment::entity::fulfillment::Delivery;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemCloseData;
use mongodb::Database;
use persistence_core::Executor;

use super::{FulfillmentTaskObject, ensure_task_matches_frozen_identity, load_single_open_task};
use crate::Result;

/// 关闭已由采购变更证明应退役的 API 直发草稿任务。
/// # 参数
/// `delivery` 必须是当前事务已核对未发货的自动草稿；`actor_id` 是变更审批执行人。
/// # 返回
/// 原任务按采购变更原因关闭，保留责任人和全部原处理历史。
/// # 错误
/// 任务缺失、重复、责任身份不符或乐观锁失败时回滚变更。
pub(crate) async fn close_api_delivery_task(
    db: &Database,
    delivery: &Delivery,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut task = load_single_open_task(db, "delivery", &delivery.base.id, executor).await?;
    ensure_task_matches_frozen_identity(&task, &FulfillmentTaskObject::Delivery(delivery))?;
    task.close(
        actor_id,
        WorkItemCloseData {
            close_reason: "采购变更替代未发 API 执行，原自动直发草稿停止办理".into()
        },
        Instant::now(),
    )?;
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}
