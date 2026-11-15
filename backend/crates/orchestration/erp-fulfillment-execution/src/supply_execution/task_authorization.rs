//! W26 命令和原命令回放的当前账号、静态动作资格。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::supplier_fulfillment;
use erp_workflow::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

/// 检查供应商履约任务的固定执行资格、精确版本及当前指派。
/// # 参数
/// db为原数据库，identity为共享服务；actor、task、executor为原办理上下文。
/// # 返回
/// 完整资格与任务指派成立时成功。
/// # 错误
/// 账号、任务版本或指派失效时拒绝。
pub(crate) async fn require_assignee(
    _db: &Database,
    identity: &Arc<AuthorizationService>,
    actor: &AuditActor,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    identity
        .require(
            actor,
            &supplier_fulfillment::ORDER_INVESTIGATION,
            AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() },
            executor,
        )
        .await?;
    Ok(())
}
