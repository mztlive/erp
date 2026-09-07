//! W26 既有资格接缝；真实任务授权仍由独立工作流读取/写入能力承担。
use crate::Result;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;
/// 沿用 W26 当前责任人的资格判定；详情投影与调查命令共享此入口。
pub async fn ensure_task_actor_eligible(
    db: &Database,
    item: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (db, item, actor_id, executor);
    Ok(())
}
