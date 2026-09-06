//! 审批任务完成、关闭与开放任务创建。

use bpm::engine::{TaskCloseReason, TaskIntent};
use bpm::ids::ApprovalNodeExecutionId;
use database::WorkItemExt;
use entities::work_item::{
    ApprovalRuntimeTaskEnding, DocumentApprovalWorkItemData, WorkItem, WorkItemPriority,
};
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use id_generator::next_id;
use mongodb::Database;

use super::super::apply_plan::PlannedWrites;
use super::{ensure_expected_version, hidden_not_found};
use crate::errors::{Error, Result};

/// 完成或关闭审批任务所需的同一决定上下文。
pub(super) struct CompleteOrCloseTasksInput<'a> {
    pub(super) complete_tasks: &'a [ApprovalNodeExecutionId],
    pub(super) close_tasks: &'a [(ApprovalNodeExecutionId, TaskCloseReason)],
    pub(super) work_item_id: &'a str,
    pub(super) expected_task_version: u64,
    pub(super) ended_execution_id: &'a str,
    pub(super) actor_id: &'a str,
    pub(super) now: Instant,
}

/// 完成或关闭当前执行对应的开放任务，CAS 保持任务版本不变式。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `input` - 当前决定涉及的任务结束事实与并发版本
/// * `session` - 调用方事务会话
///
/// # 返回
/// 当前执行对应任务完成或关闭并通过 CAS 写回时返回 `Ok(())`。
///
/// # 错误
/// 任务缺失、实体状态变更或 Repository CAS 失败时返回错误。
///
/// # 关键业务约束
/// 任务读取固定走单据审批语义 Repository，生命周期变更由 WorkItem 实体方法执行。
pub(super) async fn complete_or_close_tasks(
    db: &Database,
    input: CompleteOrCloseTasksInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let execution_id = ApprovalNodeExecutionId::new(input.ended_execution_id);
    let Some(ending) = approval_task_ending(&input, &execution_id)? else {
        return Ok(());
    };
    let tasks = db
        .work_items()
        .open_approval_tasks_for_execution(&execution_id, session)
        .await?;
    let requested = tasks
        .iter()
        .find(|item| item.base.id == input.work_item_id)
        .ok_or_else(hidden_not_found)?;
    ensure_expected_version("审批任务", input.expected_task_version, requested.base.version)?;
    let tasks =
        WorkItem::end_all_for_approval_execution(tasks, &execution_id, input.actor_id, &ending, input.now)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.work_items()
        .persist_ended_approval_tasks(&tasks, session)
        .await?;
    Ok(())
}

/// 解析当前结束执行唯一的任务终结方式。
///
/// # 错误
/// 同一执行同时被计划为完成和关闭，或出现不同关闭原因时返回冲突。
pub(super) fn approval_task_ending(
    input: &CompleteOrCloseTasksInput<'_>,
    execution_id: &ApprovalNodeExecutionId,
) -> Result<Option<ApprovalRuntimeTaskEnding>> {
    let completes = input.complete_tasks.iter().any(|item| item == execution_id);
    let close_reasons = input
        .close_tasks
        .iter()
        .filter(|(item, _)| item == execution_id)
        .map(|(_, reason)| reason.as_str())
        .collect::<Vec<_>>();
    if completes && !close_reasons.is_empty() {
        return Err(Error::ConflictError("同一审批执行同时计划完成和关闭".to_string()));
    }
    let Some(first_reason) = close_reasons.first() else {
        return Ok(completes.then_some(ApprovalRuntimeTaskEnding::Complete));
    };
    if close_reasons.iter().any(|reason| reason != first_reason) {
        return Err(Error::ConflictError(
            "同一审批执行存在不同任务关闭原因".to_string(),
        ));
    }
    Ok(Some(ApprovalRuntimeTaskEnding::Close {
        reason: (*first_reason).to_string(),
    }))
}

/// 创建开放审批任务所需的决定输出与单据责任上下文。
pub(super) struct CreateOpenTasksInput<'a> {
    pub(super) writes: &'a PlannedWrites,
    pub(super) new_task_ids: &'a [String],
    pub(super) owner_role: &'a str,
    pub(super) owner_organization_id: &'a str,
    pub(super) subject_version: &'a str,
    pub(super) business_object_id: &'a str,
    pub(super) now: Instant,
}

/// 为 `HumanTaskRequested` 意图创建新开放任务。
///
/// # 错误
/// 任务实体构造失败或 Repository 写入失败时返回错误。
pub(super) async fn create_open_tasks(
    db: &Database,
    input: CreateOpenTasksInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for (index, intent) in input.writes.create_tasks.iter().enumerate() {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(input.new_task_ids.get(index).cloned().unwrap_or_else(next_id)),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: input.writes.instance.subject.subject_kind().to_string(),
                business_object_id: input.business_object_id.to_string(),
                subject_version: input.subject_version.to_string(),
                owner_role: input.owner_role.to_string(),
                owner_organization_id: input.owner_organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            input.now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}
