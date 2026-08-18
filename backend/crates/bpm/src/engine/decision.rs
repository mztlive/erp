//! 通过与驳回的纯状态计算。

use crate::ids::ApprovalNodeExecutionId;
use crate::model::types::{
    ApprovalBlockerCode, ApprovalDecision, ApprovalExecutionAssignmentSource, ApprovalNodeExecutionStatus,
    ApprovalProcessInstanceStatus, ApprovalTerminalResult, ApprovalTransitionEvent,
};
use crate::model::{
    ApprovalNodeExecution, ApprovalProcessInstance, ApprovalTransitionDefinition, ParticipantId, Timestamp,
};

use super::enter_node::{plan_enter_node, require_decision_edges};
use super::event::{BpmEvent, BpmEventKind};
use super::transition_plan::{CommitRequired, TaskCloseReason, TaskIntent, TransitionPlan};
use super::{DefinitionGraph, Eligibility, EngineError, EngineResult};

/// 决定命令。资格结果必须由调用方预先收敛。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecideCommand {
    /// 通过或驳回。
    pub decision: ApprovalDecision,
    /// 驳回必填原因。
    pub reason: Option<String>,
    /// 决定人。
    pub actor: ParticipantId,
    /// 当前责任人写时资格。
    pub current_eligibility: Eligibility,
    /// 下一节点或入口节点资格。
    pub next_eligibility: Eligibility,
    /// 下一执行主键。
    pub next_execution_id: ApprovalNodeExecutionId,
    /// 下一执行序号。
    pub next_execution_no: u32,
    /// 调用方时间。
    pub now: Timestamp,
}

/// 对当前活动执行应用决定，并按连线进入下一节点、终态或下一轮入口。
///
/// 当前责任人失效或图损坏时不接受决定，但返回可提交的阻塞计划。
/// 下一审批人失效时保留本次通过事实。
///
/// # 错误
/// 无法形成合法快照或纯流程不变量失败时返回错误。
pub fn decide(
    instance: ApprovalProcessInstance,
    current: ApprovalNodeExecution,
    graph: &DefinitionGraph,
    command: DecideCommand,
) -> EngineResult<TransitionPlan> {
    ensure_current_token(&instance, &current)?;
    if let Some(code) = command.current_eligibility.blocked_code() {
        return block_current(instance, current, code, command.now);
    }
    let edges = match require_decision_edges(graph, &current.node_key) {
        Ok(edges) => edges,
        Err(EngineError::GraphCorrupted) => {
            return block_current(
                instance,
                current,
                ApprovalBlockerCode::DefinitionGraphCorrupted,
                command.now,
            );
        }
        Err(error) => return Err(error),
    };
    match command.decision {
        ApprovalDecision::Approve => apply_approve(instance, current, graph, edges.0, command),
        ApprovalDecision::Reject => apply_reject(instance, current, graph, edges.1, command),
    }
}

/// 通过当前节点，再进入下一节点或最终通过。
fn apply_approve(
    instance: ApprovalProcessInstance,
    mut current: ApprovalNodeExecution,
    graph: &DefinitionGraph,
    approve_edge: &ApprovalTransitionDefinition,
    command: DecideCommand,
) -> EngineResult<TransitionPlan> {
    current.record_approve(command.actor.clone(), command.reason.clone(), command.now)?;
    let mut plan = completed_current_plan(
        instance,
        current,
        command.actor.clone(),
        BpmEventKind::NodeApproved,
    );
    if approve_edge.terminal_result == Some(ApprovalTerminalResult::Approved) {
        plan.instance.complete_approved(command.now)?;
        plan.commit = CommitRequired::TerminalApproved;
        plan.events.push(BpmEvent::new(
            BpmEventKind::InstanceApproved,
            process_id(&plan.instance),
            plan.instance.current_round_no,
        ));
        return Ok(plan);
    }
    let next_key = approve_edge
        .to_node_key
        .as_deref()
        .ok_or(EngineError::GraphCorrupted)?;
    enter_after_decision(plan, graph, next_key, command, true)
}

/// 驳回当前节点，轮次加一并进入入口。
fn apply_reject(
    mut instance: ApprovalProcessInstance,
    mut current: ApprovalNodeExecution,
    graph: &DefinitionGraph,
    reject_edge: &ApprovalTransitionDefinition,
    command: DecideCommand,
) -> EngineResult<TransitionPlan> {
    let reason = command
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(EngineError::InvalidCommand("驳回原因不能为空"))?;
    ensure_reject_to_entry(graph, reject_edge)?;
    current.record_reject(command.actor.clone(), reason, command.now)?;
    instance.next_round(command.now)?;
    let mut plan = completed_current_plan(
        instance,
        current,
        command.actor.clone(),
        BpmEventKind::NodeRejected,
    );
    plan.events.push(
        BpmEvent::new(
            BpmEventKind::RoundRestarted,
            process_id(&plan.instance),
            plan.instance.current_round_no,
        )
        .with_reason(reason),
    );
    enter_after_decision(plan, graph, &graph.definition.entry_node_key, command, true)
}

/// 记录已完成的当前执行，并关闭对应任务。
fn completed_current_plan(
    instance: ApprovalProcessInstance,
    current: ApprovalNodeExecution,
    actor: ParticipantId,
    kind: BpmEventKind,
) -> TransitionPlan {
    let execution_id = execution_id(&current);
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Proceed);
    plan.events.push(
        BpmEvent::new(kind, process_id(&plan.instance), current.round_no)
            .with_execution(execution_id.clone())
            .with_node_key(current.node_key.clone())
            .with_actor(actor),
    );
    plan.task_intents.push(TaskIntent::CompleteTask { execution_id });
    plan.updated_executions.push(current);
    plan
}

/// 复用统一进入规则创建下一执行。已接受决定时保留 `Proceed`。
fn enter_after_decision(
    mut plan: TransitionPlan,
    graph: &DefinitionGraph,
    node_key: &str,
    command: DecideCommand,
    keep_commit: bool,
) -> EngineResult<TransitionPlan> {
    let source = match plan
        .created_assignees
        .iter()
        .chain(plan.updated_assignees.iter())
        .find(|item| item.node_key == node_key)
    {
        Some(binding) => binding.assignment_source.to_execution_source(),
        None => ApprovalExecutionAssignmentSource::Definition,
    };
    let enter = plan_enter_node(
        plan.instance.clone(),
        graph,
        node_key,
        plan.instance.current_round_no,
        command.next_eligibility.participant(),
        command.next_eligibility,
        command.next_execution_id,
        command.next_execution_no,
        source,
        None,
        command.now,
    )?;
    plan.merge_enter(enter, keep_commit);
    Ok(plan)
}

/// 当前责任人失效或图损坏时提交阻塞事实并关闭任务。
fn block_current(
    mut instance: ApprovalProcessInstance,
    mut current: ApprovalNodeExecution,
    code: ApprovalBlockerCode,
    now: Timestamp,
) -> EngineResult<TransitionPlan> {
    if current.status != ApprovalNodeExecutionStatus::Active {
        return Err(EngineError::Uncommittable("当前执行无法形成合法阻塞快照"));
    }
    current.block(code, now)?;
    instance.enter_blocked(code, now)?;
    let execution_id = execution_id(&current);
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Blocked);
    plan.events.push(
        BpmEvent::new(
            BpmEventKind::InstanceBlocked,
            process_id(&plan.instance),
            current.round_no,
        )
        .with_execution(execution_id.clone())
        .with_node_key(current.node_key.clone())
        .with_blocker(code),
    );
    plan.task_intents.push(TaskIntent::CloseTask {
        execution_id,
        reason: TaskCloseReason::ApprovalRuntimeBlocked,
    });
    plan.updated_executions.push(current);
    Ok(plan)
}

/// 校验执行仍是实例当前令牌且为活动态。
fn ensure_current_token(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
) -> EngineResult<()> {
    if instance.status == ApprovalProcessInstanceStatus::Approved
        || instance.status == ApprovalProcessInstanceStatus::Cancelled
    {
        return Err(EngineError::InvalidCommand("终态实例不得提交决定"));
    }
    let Some(current_id) = instance.current_node_execution_id.as_ref() else {
        return Err(EngineError::InvalidCommand("实例缺少当前执行"));
    };
    if current_id.as_ref() != current.base.id.as_str() {
        return Err(EngineError::InvalidCommand("执行不是实例当前令牌"));
    }
    if current.status != ApprovalNodeExecutionStatus::Active {
        return Err(EngineError::InvalidCommand("只有活动执行可以接受决定"));
    }
    Ok(())
}

/// 驳回连线必须指向入口。
fn ensure_reject_to_entry(
    graph: &DefinitionGraph,
    reject_edge: &ApprovalTransitionDefinition,
) -> EngineResult<()> {
    if reject_edge.event != ApprovalTransitionEvent::Reject {
        return Err(EngineError::GraphCorrupted);
    }
    if reject_edge.to_node_key.as_deref() != Some(graph.definition.entry_node_key.as_str()) {
        return Err(EngineError::GraphCorrupted);
    }
    Ok(())
}

fn process_id(instance: &ApprovalProcessInstance) -> crate::ids::ApprovalProcessInstanceId {
    crate::ids::ApprovalProcessInstanceId::new(instance.base.id.clone())
}

fn execution_id(execution: &ApprovalNodeExecution) -> ApprovalNodeExecutionId {
    ApprovalNodeExecutionId::new(execution.base.id.clone())
}
