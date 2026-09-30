//! 审批任务只读入口复用冻结运行责任链，不依赖普通业务列表范围。

use application_core::AuditActor;
use bpm::model::types::ApprovalNodeExecutionStatus;
use mongodb::Database;
use persistence_core::Executor;

use super::load_exact_runtime_snapshot;
use super::read_auth::{RuntimeReadSubject, task_proves_current_responsibility};
use crate::entity::work_item::{AssignmentSource, WorkItem, WorkItemStatus, WorkItemType};
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::prelude::*;
use crate::repository::{BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::{
    approval_action_roles_with_executor, approval_participant_permissions_with_executor,
    require_approval_management_with_executor,
};

/// 验证任务、执行、实例及冻结主体，形成仅该审批任务的读取资格。
///
/// # 参数
/// * `db` / `auth` - 工作流仓储及当前权限事实。
/// * `actor` - 当前已认证读者。
/// * `item` - 服务端读取的审批任务。
/// * `executor` - 当前查询或命令事务。
/// # 返回
/// 当前任务负责人或实际完成该任务的人返回 true；不授予业务单据详情权限。
/// # 错误
/// 仓储、策略或冻结主体损坏时失败关闭。
pub async fn approval_task_readable_with_executor(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    item: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if item.work_item_type != WorkItemType::DocumentApproval
        || item.assignment_source != AssignmentSource::ApprovalRuntime
        || approval_action_roles_with_executor(auth, actor, "approval_instance:read", executor)
            .await?
            .is_empty()
    {
        return Ok(false);
    }
    let Some(execution_id) = &item.approval_node_execution_id else {
        return Ok(false);
    };
    let Some(execution) = db.bpm_workflow().find_execution_by_id(execution_id, executor).await? else {
        return Ok(false);
    };
    let Some(instance) =
        db.bpm_workflow().find_instance_by_id(&execution.process_instance_id, executor).await?
    else {
        return Ok(false);
    };
    let (document_type, snapshot) = load_exact_runtime_snapshot(db, &instance, executor, false).await?;
    let subject =
        RuntimeReadSubject { instance, current_execution: Some(execution.clone()), snapshot, document_type };
    let owner_role = adapter_spec_of(document_type)?.owner_role;
    let owner = execution.assignee_participant_id.as_str();
    let exact_chain = if item.status == WorkItemStatus::Open {
        let tasks = db.work_items().open_approval_tasks_for_execution(execution_id, executor).await?;
        tasks.len() == 1
            && tasks[0].base.id == item.base.id
            && task_proves_current_responsibility(item, &execution, &subject, owner, owner_role.as_str())
    } else {
        terminal_task_matches_subject(item, &subject, owner, owner_role.as_str())
    };
    if !exact_chain {
        return Ok(false);
    }
    if owner == actor.id()
        && (item.status != WorkItemStatus::Open
            || approval_participant_permissions_with_executor(auth, actor, executor).await?)
    {
        return Ok(true);
    }
    manager_can_read(auth, actor, &subject, executor).await
}

/// 管理员在真实业务来源边界内读取已验证的审批任务。
async fn manager_can_read(
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    subject: &RuntimeReadSubject,
    executor: &mut dyn Executor,
) -> Result<bool> {
    match require_approval_management_with_executor(
        auth,
        actor,
        "approval_instance:read",
        subject.document_type,
        &subject.snapshot.business_object_id,
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
fn terminal_task_matches_subject(
    item: &WorkItem,
    subject: &RuntimeReadSubject,
    owner: &str,
    owner_role: &str,
) -> bool {
    let Some(execution) = &subject.current_execution else {
        return false;
    };
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
