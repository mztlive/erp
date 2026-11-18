//! 取消当前执行与实例。

use super::event::{BpmEvent, BpmEventKind};
use super::transition_plan::{CommitRequired, TaskCloseReason, TaskIntent, TransitionPlan};
use super::{EngineError, EngineResult};
use crate::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use crate::model::{ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId, Timestamp};

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

/// 在调用方完成撤回授权、开放任务核验和阻塞分类后，计算实例与当前执行一并取消的计划。
/// 计划清除当前执行引用，发出 `InstanceCancelled`，并按命令决定是否加入任务关闭意图；不会自行判断该走业务取消还是受阻取消。
///
/// # 错误
/// 终态、缺失或不匹配的当前令牌、不可结束的执行、空白事由或模型不变式失败均不能提交。
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

    let blocker = instance.blocker_code.or(current.blocker_code);
    current.cancel(command.now)?;
    instance.cancel(command.now)?;

    let execution_id = current.typed_id();
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Cancelled);
    let mut event =
        BpmEvent::new(BpmEventKind::InstanceCancelled, plan.instance.typed_id(), current.round_no)
            .with_execution(execution_id.clone())
            .with_actor(command.actor)
            .with_reason(reason);
    if let Some(code) = blocker {
        event = event.with_blocker(code);
    }
    plan.events.push(event);

    if command.close_open_task {
        plan.task_intents.push(TaskIntent::CloseTask { execution_id, reason: TaskCloseReason::Cancelled });
    }
    plan.updated_executions.push(current);
    Ok(plan)
}

// 在生成取消快照前，确认实例仍在运行或受阻，且传入的是可结束的当前执行，避免用旧令牌撤回实例。
fn ensure_cancellable(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
) -> EngineResult<()> {
    match instance.status {
        ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked => {},
        ApprovalProcessInstanceStatus::Approved
        | ApprovalProcessInstanceStatus::Rejected
        | ApprovalProcessInstanceStatus::Cancelled => {
            return Err(EngineError::InvalidCommand("终态实例不得取消"));
        },
    }
    if !matches!(current.status, ApprovalNodeExecutionStatus::Active | ApprovalNodeExecutionStatus::Blocked) {
        return Err(EngineError::Uncommittable("当前执行状态无法形成合法取消计划"));
    }

    let Some(current_id) = instance.current_node_execution_id.as_ref() else {
        return Err(EngineError::Uncommittable("实例缺少当前执行，无法形成合法取消计划"));
    };
    if current_id.as_ref() != current.base.id.as_str() {
        return Err(EngineError::InvalidCommand("执行不是实例当前令牌"));
    }
    Ok(())
}
