//! 审批任务只读入口复用冻结运行责任链，不依赖普通业务列表范围。

use bpm::model::ApprovalNodeExecution;
use bpm::model::types::ApprovalNodeExecutionStatus;

use super::read_auth::{RuntimeReadSubject, task_proves_current_responsibility};
use crate::entity::work_item::{WorkItem, WorkItemStatus};
use crate::error::Result;
use crate::service::approval::business_adapter::adapter_spec_of;

mod batch;
pub(crate) use batch::approval_task_facts_current;
#[cfg(test)]
mod tests;

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
