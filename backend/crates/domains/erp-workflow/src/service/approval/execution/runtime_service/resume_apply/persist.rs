//! 同一执行器内按固定顺序应用恢复计划。

use bpm::ids::ApprovalNodeExecutionId;
use mongodb::Database;
use persistence_core::Executor;

use super::super::super::idempotency::map_receipt_first_write_error;
use super::super::notifications::{ResumeNotificationFacts, persist_resume_notifications};
use super::super::require_cas_applied;
use super::super::tasks::{CreateOpenTasksInput, create_open_tasks};
use super::prepare::{ClosedTaskGuard, PreparedResume};
use crate::entity::work_item::WorkItemStatus;
use crate::error::{Error, Result};
use crate::ports::WorkflowAuditPort;
use crate::repository::prelude::*;
use crate::repository::{BpmExt, WorkItemExt};

/// 在调用方唯一事务内保存正式事实，旧关闭任务始终保持不可变。
///
/// # 参数
/// * `db` - 工作流数据库
/// * `prepared` - 恢复计划、CAS 版本和冻结任务、通知及审计元数据
/// * `audit_port` - 写入同一事务的审计端口
/// * `executor` - 调用方持有的事务执行器
///
/// # 返回
/// 全部恢复事实写入时返回 `Ok(())`。
///
/// # 错误
/// 历史任务守卫、CAS、计划不变式、唯一索引或写入失败时立即停止并由调用方回滚。
///
/// # 关键业务约束
/// 收据是只读验证后的第一笔物理写；其后依次推进实例、结束旧执行、
/// 创建新执行及任务、写通知 outbox，最后记录审计。
pub(super) async fn persist_resume_writes(
    db: &Database,
    prepared: &PreparedResume,
    audit_port: &dyn WorkflowAuditPort,
    executor: &mut dyn Executor,
) -> Result<()> {
    let [new_execution] = prepared.writes.created_executions.as_slice() else {
        return Err(Error::Internal("原审批人恢复必须且只能创建一个新执行".to_string()));
    };
    verify_closed_resume_task(db, prepared.closed_task_guard.as_ref(), executor).await?;
    db.bpm_workflow()
        .insert_command_receipt(&prepared.writes.receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    advance_resume_instance(db, prepared, executor).await?;
    end_replaced_execution(db, prepared, executor).await?;
    if !prepared.writes.created_assignees.is_empty() {
        return Err(Error::Internal("原审批人恢复不得修改实例审批人绑定".to_string()));
    }
    db.bpm_workflow().insert_execution(new_execution, executor).await?;
    persist_resume_tasks(db, prepared, executor).await?;
    persist_resume_notifications(
        db,
        &prepared.writes,
        ResumeNotificationFacts {
            new_execution,
            submitted_by: &prepared.snapshot.payload.submitted_by,
            document_type_label: &prepared.document_type_label,
            document_no: &prepared.snapshot.payload.document_no,
        },
        prepared.now,
        executor,
    )
    .await?;
    audit_port.persist(&prepared.audit, executor).await?;
    Ok(())
}

/// 原关闭任务的状态、版本与所属执行必须仍匹配准备阶段。
async fn verify_closed_resume_task(
    db: &Database,
    guard: Option<&ClosedTaskGuard>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(guard) = guard else {
        return Ok(());
    };
    let task = db
        .work_items()
        .find_document_approval_by_id(&guard.task_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("原关闭审批任务不存在".to_string()))?;
    if task.status != WorkItemStatus::Closed
        || task.base.version != guard.version
        || task.approval_node_execution_id.as_ref() != Some(&guard.execution_id)
    {
        return Err(Error::version_conflict("原关闭审批任务"));
    }
    Ok(())
}

/// 保持实例 CAS 的原执行指针和列表投影一致。
async fn advance_resume_instance(
    db: &Database,
    prepared: &PreparedResume,
    executor: &mut dyn Executor,
) -> Result<()> {
    let expected_execution_id = ApprovalNodeExecutionId::new(&prepared.ended_execution_id);
    require_cas_applied(
        db.bpm_workflow()
            .advance_instance(
                &prepared.writes.instance,
                prepared.expected_instance_version,
                &expected_execution_id,
                &prepared.list_projection,
                executor,
            )
            .await?,
        "审批实例",
    )
}

/// 只允许按客户端期望版本结束原受阻执行。
async fn end_replaced_execution(
    db: &Database,
    prepared: &PreparedResume,
    executor: &mut dyn Executor,
) -> Result<()> {
    for execution in &prepared.writes.updated_executions {
        if execution.base.id != prepared.ended_execution_id {
            return Err(Error::Internal("恢复计划包含非当前旧执行更新".to_string()));
        }
        require_cas_applied(
            db.bpm_workflow()
                .end_blocked_execution(execution, prepared.expected_execution_version, executor)
                .await?,
            "受阻审批执行",
        )?;
    }
    Ok(())
}

/// 新任务仅绑定新执行，业务责任及版本全部取自冻结主体。
async fn persist_resume_tasks(
    db: &Database,
    prepared: &PreparedResume,
    executor: &mut dyn Executor,
) -> Result<()> {
    create_open_tasks(
        db,
        CreateOpenTasksInput {
            writes: &prepared.writes,
            new_task_ids: &prepared.new_task_ids,
            owner_role: prepared.spec.owner_role.as_str(),
            owner_organization_id: &prepared.snapshot.payload.responsible_org_id,
            subject_version: &prepared.subject_version,
            business_object_id: &prepared.snapshot.business_object_id,
            now: prepared.now,
        },
        executor,
    )
    .await
}
