//! 采购创建任务由固定授权入口核验身份、版本及实际分配。
mod query;
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::purchase_order;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;
pub(crate) use query::query_tasks;

pub(crate) use super::repository::creation_tasks::CreationTaskFilter;
use crate::Result;

/// 检查当前任务版本及采购创建入口资格。
/// # 参数
/// db 为业务数据库，authorization 为共享服务，task 为同事务读取的精确任务。
/// # 返回
/// 固定入口的任务与来源条件都满足时成功。
/// # 错误
/// 账号、任务、版本、来源或身份设施失败原样返回。
pub async fn require_creation_task(
    _db: &Database,
    authorization: &Arc<AuthorizationService>,
    actor: &AuditActor,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(
            actor,
            &purchase_order::CREATE,
            AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() },
            executor,
        )
        .await?;
    Ok(())
}

pub use super::repository::creation_tasks::CreationTaskMapping;
