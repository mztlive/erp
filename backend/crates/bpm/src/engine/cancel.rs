//! 取消当前执行与实例。

use crate::model::types::{ApprovalBlockerCode, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use crate::model::{ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId, Timestamp};

use super::event::{BpmEvent, BpmEventKind};
use super::transition_plan::{CommitRequired, TaskCloseReason, TaskIntent, TransitionPlan};
use super::{EngineError, EngineResult};

/// 取消命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelCommand {
    /// 取消人。
    pub actor: ParticipantId,
    /// 非空撤回原因。
    pub reason: String,
    /// 当前执行曾产生开放任务时为真。
    pub close_open_task: bool,
    /// 调用方时间。
    pub now: Timestamp,
}

/// 将当前执行与实例置为 `CANCELLED`，并清空当前执行引用。
///
/// 本函数不区分业务撤回与受阻取消；调用方必须先完成权限与 blocker 分类校验。
/// 无法形成合法取消快照时返回不可提交错误。
///
/// # 错误
/// 终态、缺少当前执行或原因非法时返回错误。
pub fn cancel(
    mut instance: ApprovalProcessInstance,
    mut current: ApprovalNodeExecution,
    command: CancelCommand,
) -> EngineResult<TransitionPlan> {
    ensure_cancellable(&instance, &current)?;
    let reason = command.reason.trim();
    if reason.is_empty() {
        return Err(EngineError::InvalidCommand("取消原因不能为空"));
    }
    current.cancel(command.now)?;
    instance.cancel(command.now)?;
    let execution_id = crate::ids::ApprovalNodeExecutionId::new(current.base.id.clone());
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Cancelled);
    plan.events.push(
        BpmEvent::new(
            BpmEventKind::InstanceCancelled,
            crate::ids::ApprovalProcessInstanceId::new(plan.instance.base.id.clone()),
            current.round_no,
        )
        .with_execution(execution_id.clone())
        .with_actor(command.actor)
        .with_reason(reason),
    );
    if command.close_open_task {
        plan.task_intents.push(TaskIntent::CloseTask {
            execution_id,
            reason: TaskCloseReason::Cancelled,
        });
    }
    plan.updated_executions.push(current);
    Ok(plan)
}

/// 运行中或受阻实例才允许取消，且必须存在可结束的当前执行。
fn ensure_cancellable(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
) -> EngineResult<()> {
    if instance.status.is_terminal() {
        return Err(EngineError::InvalidCommand("终态实例不得取消"));
    }
    if !matches!(
        instance.status,
        ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked
    ) {
        return Err(EngineError::InvalidCommand("只有运行中或受阻实例可以取消"));
    }
    if current.status.is_ended() {
        return Err(EngineError::Uncommittable("当前执行已结束，无法形成合法取消计划"));
    }
    if current.status != ApprovalNodeExecutionStatus::Active
        && current.status != ApprovalNodeExecutionStatus::Blocked
    {
        return Err(EngineError::Uncommittable("当前执行状态无法形成合法取消计划"));
    }
    let Some(current_id) = instance.current_node_execution_id.as_ref() else {
        return Err(EngineError::Uncommittable(
            "实例缺少当前执行，无法形成合法取消计划",
        ));
    };
    if current_id.as_ref() != current.base.id.as_str() {
        return Err(EngineError::InvalidCommand("执行不是实例当前令牌"));
    }
    let _ = ApprovalBlockerCode::InternalInvariantBroken;
    Ok(())
}
