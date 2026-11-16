//! 财务写命令先以真实账号核验当前任务，再读取回执与业务对象。
use application_core::AuditActor;
use erp_core::ids::WorkItemId;
use erp_workflow::WorkItemExt;
use mongodb::Database;
use persistence_core::Executor;

use super::task_access::{TaskPurpose, require_task};
use crate::{Error, FinancePostingPorts, Result};

/// 在当前命令事务内核验真实任务的当前版本，允许已提交命令沿原任务重放。
/// # 参数
/// db 为业务数据库，ports 为共享授权设施，actor 为当前认证操作人，
/// task_id 为原任务主键，purpose 为固定动作，executor 为原命令事务执行器。
/// # 返回
/// 当前账号仍具备同一任务办理资格时成功。
/// # 错误
/// 任务缺失时返回 `NotFound`；资格、任务版本或指派失效时拒绝。
/// 授权及仓储读取错误按原分类传播。
pub(crate) async fn require_entry(
    db: &Database,
    ports: &FinancePostingPorts,
    actor: &AuditActor,
    task_id: &WorkItemId,
    purpose: TaskPurpose,
    executor: &mut dyn Executor,
) -> Result<()> {
    let task = db
        .work_items()
        .find_by_id(task_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("财务办理任务不存在".into()))?;
    require_task(db, ports, actor, &task, purpose, executor).await
}
