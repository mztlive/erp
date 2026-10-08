//! W26 既有资格接缝；真实任务授权仍由独立工作流读取/写入能力承担。
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
/// 详情投影与调查命令共享的资格入口。当前实现不读取参数，直接成功。
///
/// # 参数
/// * `db` - 保留的数据库，当前未使用。
/// * `item` - 保留的任务，当前未使用。
/// * `actor_id` - 保留的操作人，当前未使用。
/// * `executor` - 保留的执行器，当前未使用。
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 不返回错误。
pub async fn ensure_task_actor_eligible(
    db: &Database,
    item: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (db, item, actor_id, executor);
    Ok(())
}
