//! 决定命令在收据之后的领域写入。
//!
//! 调用方必须先写入命令收据；本模块不再重复写收据，也不调整既有写序。

use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::ApprovalNodeExecution;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use super::super::super::apply_plan::PlannedWrites;
use super::super::notifications::{DecisionNotificationFacts, persist_decision_notifications};
use super::super::require_cas_applied;
use super::super::tasks::{
    CompleteOrCloseTasksInput, CreateOpenTasksInput, complete_or_close_tasks, create_open_tasks,
};
use crate::error::{Error, Result};
use crate::ports::PreparedWorkflowAudit;
use crate::repository::BpmExt;
use crate::repository::bpm::ApprovalInstanceListProjection;

/// 决定写库事务所需的计划、版本守卫与通知事实。
///
/// # 用途
/// 打包 [`persist_decision_writes`] 的非基础设施参数。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 调用方必须先写入命令收据，再传入本结构执行领域写。
pub(super) struct PersistDecisionWrites<'a> {
    pub(super) writes: &'a PlannedWrites,
    pub(super) ended_execution_id: &'a str,
    pub(super) expected_instance_version: u64,
    pub(super) expected_execution_version: u64,
    pub(super) expected_task_version: u64,
    pub(super) work_item_id: &'a str,
    pub(super) new_task_ids: &'a [String],
    pub(super) list_projection: &'a ApprovalInstanceListProjection,
    pub(super) audit: &'a PreparedWorkflowAudit,
    pub(super) audit_port: &'a dyn crate::ports::WorkflowAuditPort,
    pub(super) now: Instant,
    pub(super) actor_id: &'a str,
    pub(super) owner_role: &'a str,
    pub(super) owner_organization_id: &'a str,
    pub(super) subject_version: &'a str,
    pub(super) business_object_id: &'a str,
    pub(super) document_type_label: &'a str,
    pub(super) document_no: &'a str,
    pub(super) submitted_by: &'a str,
    pub(super) ended_execution: &'a ApprovalNodeExecution,
    pub(super) reject_reason: Option<&'a str>,
    pub(super) runtime_admin_ids: &'a [String],
}

/// 单事务应用决定写入：实例 CAS 推进、执行结束/插入、审批人绑定、任务
/// 完成/关闭/新建、通知 outbox 与审计。
///
/// # 用途
/// 在调用方已写入命令收据后，持久化决定计划的全部领域副作用。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 计划、版本守卫、任务与通知事实
/// * `executor` - 执行器
///
/// # 返回
/// 全部写入成功时返回 `Ok(())`。
///
/// # 错误
/// CAS 冲突、任务缺失、通知意图非法或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 命令收据必须由调用方先写入以仲裁并发；本函数不再重复写收据。
pub(super) async fn persist_decision_writes(
    db: &Database,
    input: PersistDecisionWrites<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    persist_decision_runtime_facts(db, &input, executor).await?;
    persist_decision_task_writes(db, &input, executor).await?;
    persist_decision_side_effects(db, &input, executor).await?;
    Ok(())
}

async fn persist_decision_runtime_facts(
    db: &Database,
    input: &PersistDecisionWrites<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    // 实例 CAS 推进（RUNNING|BLOCKED + 当前执行不变式）。期望版本为加载时的
    // 持久化版本（引擎计划内的版本已在快照上自增），未应用视为并发冲突。
    let expected_current_execution_id = ApprovalNodeExecutionId::new(input.ended_execution_id);
    require_cas_applied(
        db.bpm_workflow()
            .advance_instance(
                &input.writes.instance,
                input.expected_instance_version,
                &expected_current_execution_id,
                input.list_projection,
                executor,
            )
            .await?,
        "审批实例",
    )?;
    // 当前执行结束（APPROVED/REJECTED/BLOCKED）。
    for execution in &input.writes.updated_executions {
        let expected = if execution.base.id.as_str() == input.ended_execution_id {
            input.expected_execution_version
        } else {
            execution
                .base
                .version
                .checked_sub(1)
                .ok_or_else(|| Error::Internal("决定后执行版本非法".to_string()))?
        };
        require_cas_applied(
            db.bpm_workflow().end_active_execution(execution, expected, executor).await?,
            "审批执行",
        )?;
    }
    // 新执行与审批人绑定。
    for execution in &input.writes.created_executions {
        db.bpm_workflow().insert_execution(execution, executor).await?;
    }
    if !input.writes.created_assignees.is_empty() {
        db.bpm_workflow().insert_assignees(&input.writes.created_assignees, executor).await?;
    }
    Ok(())
}

async fn persist_decision_task_writes(
    db: &Database,
    input: &PersistDecisionWrites<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    // 任务：完成当前、按原因关闭、为下一节点新建。
    complete_or_close_tasks(
        db,
        CompleteOrCloseTasksInput {
            complete_tasks: &input.writes.complete_tasks,
            close_tasks: &input.writes.close_tasks,
            work_item_id: input.work_item_id,
            expected_task_version: input.expected_task_version,
            ended_execution_id: input.ended_execution_id,
            actor_id: input.actor_id,
            now: input.now,
        },
        executor,
    )
    .await?;
    create_open_tasks(
        db,
        CreateOpenTasksInput {
            writes: input.writes,
            new_task_ids: input.new_task_ids,
            owner_role: input.owner_role,
            owner_organization_id: input.owner_organization_id,
            subject_version: input.subject_version,
            business_object_id: input.business_object_id,
            now: input.now,
        },
        executor,
    )
    .await?;
    Ok(())
}

async fn persist_decision_side_effects(
    db: &Database,
    input: &PersistDecisionWrites<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    persist_decision_notifications(
        db,
        input.writes,
        DecisionNotificationFacts {
            document_type_label: input.document_type_label,
            document_no: input.document_no,
            submitted_by: input.submitted_by,
            ended_execution: input.ended_execution,
            reject_reason: input.reject_reason,
            runtime_admin_ids: input.runtime_admin_ids,
        },
        input.now,
        executor,
    )
    .await?;
    input.audit_port.persist(input.audit, executor).await?;
    Ok(())
}
