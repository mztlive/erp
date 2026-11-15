//! 审批任务只读入口复用冻结运行责任链，不依赖普通业务列表范围。

use application_core::AuditActor;
use bpm::model::ApprovalNodeExecution;
use bpm::model::types::ApprovalNodeExecutionStatus;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_permissions::internal as internal_permissions;
use mongodb::Database;
use persistence_core::Executor;

use super::read_auth::{RuntimeReadSubject, task_proves_current_responsibility};
use crate::entity::work_item::{WorkItem, WorkItemStatus};
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::require_approval_management_with_executor;

mod batch;
pub use batch::approval_tasks_readable_with_executor;
#[cfg(test)]
mod tests;

/// 验证任务、执行、实例及冻结主体，形成仅该审批任务的读取资格。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `db` / `auth` - 工作流仓储及当前权限事实。
/// * `actor` - 当前已认证读者。
/// * `item` - 服务端读取的审批任务。
/// * `executor` - 当前查询或命令事务。
/// # 返回
/// 当前任务负责人或实际完成该任务的人返回 true；不授予业务单据详情权限。
/// # 错误
/// 仓储、策略或冻结主体损坏时失败关闭。
pub async fn approval_task_readable_with_executor(
    authorization: &AuthorizationService,
    identity_service: &SharedRbacService,
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    item: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let readable = approval_tasks_readable_with_executor(
        authorization,
        identity_service,
        db,
        auth,
        actor,
        &[item],
        executor,
    )
    .await?;
    Ok(readable.first().copied().unwrap_or(false))
}

/// 对单任务和批量任务共用同一开放唯一链及实际完成链规则。
///
/// # 参数
/// * `item` / `subject` - 精确任务和冻结主体
/// * `open_tasks` - 当前执行的全部开放任务，终态时不使用
///
/// # 返回
/// 同一任务责任、岗位、组织、节点与冻结提交主体成立时返回 true。
///
/// # 错误
/// 业务审批适配器未登记时保持原错误。
fn task_chain_matches_subject(
    item: &WorkItem,
    subject: &RuntimeReadSubject,
    open_tasks: &[WorkItem],
) -> Result<bool> {
    let Some(execution) = &subject.current_execution else {
        return Ok(false);
    };
    let owner_role = adapter_spec_of(subject.document_type)?.owner_role;
    let owner = item.owner_user_id.as_deref().unwrap_or_default();
    Ok(if item.status == WorkItemStatus::Open {
        super::read_auth::open_tasks_match(execution, open_tasks)
            && open_tasks.iter().any(|task| task.base.id == item.base.id)
            && task_proves_current_responsibility(item, execution, subject, owner, owner_role.as_str())
    } else {
        terminal_task_matches_subject(item, subject, execution, owner, owner_role.as_str())
    })
}

/// 管理员在真实业务来源边界内读取已验证的审批任务。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `auth` / `actor` - 当前权限端口与身份
/// * `subject` - 已验证冻结主体
/// * `executor` - 原事务执行器
///
/// # 返回
/// 原管理资格和真实来源都成立时返回 true，权限拒绝返回 false。
///
/// # 错误
/// 除 Forbidden 之外的策略及来源错误原样传播。
async fn manager_can_read(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    subject: &RuntimeReadSubject,
    executor: &mut dyn Executor,
) -> Result<bool> {
    match require_approval_management_with_executor(
        authorization,
        actor,
        &internal_permissions::APPROVAL_MANAGEMENT_READ,
        subject.document_type,
        &subject.instance.base.id,
        executor,
    )
    .await
    {
        Ok(()) => Ok(true),
        Err(Error::Forbidden(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

/// 已完成任务只承认同一次实际决定与冻结提交主体。
///
/// # 参数
/// * `item` / `subject` - 完成任务与冻结运行事实
/// * `execution` - 责任链校验入口已确认存在的执行事实
/// * `owner` / `owner_role` - 实际执行人及业务规定岗位
///
/// # 返回
/// 原终态、完成人、决定事实与主体均相同时返回 true。
///
/// # 错误
/// 无。
fn terminal_task_matches_subject(
    item: &WorkItem,
    subject: &RuntimeReadSubject,
    execution: &ApprovalNodeExecution,
    owner: &str,
    owner_role: &str,
) -> bool {
    if item.status == WorkItemStatus::Closed
        && item.close_reason.as_deref() == Some("APPROVAL_OR_SIGN_RESOLVED")
    {
        return execution.has_participant(owner)
            && item.approval_node_execution_id.as_ref().is_some_and(|id| id.as_ref() == execution.base.id)
            && execution
                .decided_by
                .as_ref()
                .is_some_and(|actor| item.closed_by.as_deref() == Some(actor.as_str()))
            && execution.status.is_ended()
            && item.business_object_id == subject.snapshot.business_object_id
            && item.business_object_type == subject.document_type.as_str()
            && item.subject_version == subject.instance.subject_version.to_string()
            && item.owner_organization_id == subject.snapshot.payload.responsible_org_id
            && item.owner_role == owner_role;
    }
    item.status == WorkItemStatus::Completed
        && item.completed_by.as_deref() == Some(owner)
        && item.owner_user_id.as_deref() == Some(owner)
        && item.owner_role == owner_role
        && item.owner_organization_id == subject.snapshot.payload.responsible_org_id
        && item.business_object_type == subject.document_type.as_str()
        && item.business_object_id == subject.snapshot.business_object_id
        && item.subject_version == subject.snapshot.subject_version.to_string()
        && execution.decided_by.as_ref().is_some_and(|participant| participant.as_str() == owner)
        && execution.decided_at.is_some()
        && matches!(
            execution.status,
            ApprovalNodeExecutionStatus::Approved | ApprovalNodeExecutionStatus::Rejected
        )
}
