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
    require_target(identity, actor, &task.base.id, task.base.version, executor).await
}

/// 在业务任务和版本读取前，用命令中的精确任务目标执行统一授权。
/// # 参数
/// identity 为共享授权服务，actor、id、version 为当前命令身份及预期任务版本。
/// # 返回
/// 当前完整资格和精确任务目标成立时成功。
/// # 错误
/// 身份、指派、任务版本、来源或执行边界无效时拒绝。
pub(super) async fn require_target(
    identity: &AuthorizationService,
    actor: &AuditActor,
    id: &str,
    version: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    identity
        .require(
            actor,
            &supplier_fulfillment::ORDER_INVESTIGATION,
            AccessTarget::Task { id: id.to_owned(), version: version.to_string() },
            executor,
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    #[tokio::test]
    async fn precise_command_target_requires_current_qualification_before_business_version_reads() {
        let identity = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let error =
            require_target(&identity, &actor, "task", 3, &mut SnapshotExecutor::default()).await.unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        let error =
            require_target(&identity, &actor, "task", 3, &mut ReadExecutor::default()).await.unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务"));
    }
}
