//! 复核重放通过固定入口重验精确任务身份及当前指派。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::supplier_settlement;
use erp_workflow::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

/// 在原事务核验原复核任务的当前责任和完整功能资格。
/// # 参数
/// db 为业务数据库；authorization 为共享服务；task 是原事务读取的任务。
/// # 返回
/// 精确任务版本、当前指派与确认资格都有效时成功。
/// # 错误
/// 授权、版本及事实读取失败原样传播。
pub async fn require_reviewer(
    _db: &Database,
    authorization: &Arc<AuthorizationService>,
    actor: &AuditActor,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(
            actor,
            &supplier_settlement::STATEMENT_REVIEW_DECIDE,
            AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() },
            executor,
        )
        .await?;
    Ok(())
}
