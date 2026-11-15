//! 付款跟进按共享服务检查精确任务及当前指派。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::purchase_payment_request;
use erp_workflow::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

/// 检查原入口资格、任务版本及当前指定人。
/// # 参数
/// db为原数据库；authorization为共享服务；actor、task、executor为原事务上下文。
/// # 返回
/// 完整授权成立时成功。
/// # 错误
/// 资格、任务或指派失效时拒绝。
pub(super) async fn require_assigned(
    _db: &Database,
    authorization: &Arc<AuthorizationService>,
    actor: &AuditActor,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(
            actor,
            &purchase_payment_request::SUBMIT,
            AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() },
            executor,
        )
        .await?;
    Ok(())
}
