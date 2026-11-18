//! 通过与驳回的纯状态计算。

use super::enter_node::{EnterNodeInput, plan_enter_node, require_decision_edges};
use super::event::{BpmEvent, BpmEventKind};
use super::transition_plan::{CommitRequired, TaskCloseReason, TaskIntent, TransitionPlan};
use super::{DefinitionGraph, Eligibility, EngineError, EngineResult};
use crate::ids::ApprovalNodeExecutionId;
use crate::model::types::{
    ApprovalBlockerCode, ApprovalDecision, ApprovalExecutionAssignmentSource, ApprovalNodeExecutionStatus,
    ApprovalTerminalResult, ApprovalTransitionEvent,
};
use crate::model::{
    ApprovalNodeExecution, ApprovalProcessInstance, ApprovalTransitionDefinition, ParticipantId, Timestamp,
};

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
    /// 通过后的下一节点资格；驳回不读取此资格。
    pub next_eligibility: Eligibility,
    /// 下一执行主键。
    pub next_execution_id: ApprovalNodeExecutionId,
    /// 下一执行序号。
    pub next_execution_no: u32,
    /// 调用方时间。
    pub now: Timestamp,
}

/// 收到审批决定后，按冻结图推进当前活动执行。通过时进入下一节点或最终通过，驳回则终结本实例。
/// 单人节点当前人员失效或决定连线缺失、重复时，不接受决定，改为返回应提交的 `CommitRequired::Blocked`；
/// 或签节点仅拒绝失去资格的决定人。若通过已接受而下一节点无有效办理人，计划保留通过事实并记录下一节点受阻。
///
/// # 错误
/// 终态、令牌或参与人不匹配、驳回事由为空、连线形状非法、缺少通过目标节点等无法形成合法迁移的情况返回错误。
pub fn decide(
    instance: ApprovalProcessInstance,
    current: ApprovalNodeExecution,
    graph: &DefinitionGraph,
    command: DecideCommand,
) -> EngineResult<TransitionPlan> {
    ensure_current_token(&instance, &current)?;
    ensure_actor(&current, graph, &command)?;
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
        },
        Err(error) => return Err(error),
    };

    match command.decision {
        ApprovalDecision::Approve => apply_approve(instance, current, graph, edges.0, command),
        ApprovalDecision::Reject => apply_reject(instance, current, graph, edges.1, command),
    }
}

/// 供只允许驳回的入口计算决定：接受后终止实例，不产生新轮次或新任务。
/// 沿用 [`decide`] 的当前令牌、资格和冻结图校验；非驳回命令直接拒绝，单人资格失效或当前连线缺失、重复仍可返回阻塞计划。
pub fn reject_terminal(
    instance: ApprovalProcessInstance,
    current: ApprovalNodeExecution,
    graph: &DefinitionGraph,
    command: DecideCommand,
) -> EngineResult<TransitionPlan> {
    if command.decision != ApprovalDecision::Reject {
        return Err(EngineError::InvalidCommand("终态驳回只接受驳回命令"));
    }
    decide(instance, current, graph, command)
}

fn ensure_actor(
    current: &ApprovalNodeExecution,
    graph: &DefinitionGraph,
    command: &DecideCommand,
) -> EngineResult<()> {
    let node = graph.node(&current.node_key).ok_or(EngineError::GraphCorrupted)?;
    if current.assignee_participant_id != *node.assignee()?
        || current.additional_assignees != node.additional_assignees
    {
        return Err(EngineError::InvalidCommand("执行参与人必须匹配冻结定义"));
    }

    if !current.has_participant(command.actor.as_str())
        || command.current_eligibility.participant_ref() != &command.actor
        || matches!(command.current_eligibility, Eligibility::AnyOf { .. })
    {
        return Err(EngineError::InvalidCommand("决定人及资格必须属于当前节点"));
    }
    if !current.additional_assignees.is_empty() && command.current_eligibility.blocked_code().is_some() {
        return Err(EngineError::InvalidCommand("当前或签审批人资格已失效"));
    }
    Ok(())
}

// 当前决定通过校验后先记下通过事实；末节点收结实例，其余节点在同轮交接给下一执行。
fn apply_approve(
    instance: ApprovalProcessInstance,
    mut current: ApprovalNodeExecution,
    graph: &DefinitionGraph,
    approve_edge: &ApprovalTransitionDefinition,
    mut command: DecideCommand,
) -> EngineResult<TransitionPlan> {
    current.record_approve(command.actor.clone(), command.reason.take(), command.now)?;
    let mut plan =
        completed_current_plan(instance, current, command.actor.clone(), BpmEventKind::NodeApproved);

    if approve_edge.terminal_result == Some(ApprovalTerminalResult::Approved) {
        plan.instance.complete_approved(command.now)?;
        plan.commit = CommitRequired::TerminalApproved;
        plan.events.push(BpmEvent::new(
            BpmEventKind::InstanceApproved,
            plan.instance.typed_id(),
            plan.instance.current_round_no,
        ));
        return Ok(plan);
    }

    let next_key = approve_edge.to_node_key.as_deref().ok_or(EngineError::GraphCorrupted)?;
    enter_after_decision(plan, graph, next_key, command)
}

// 在驳回路径校验事由和冻结连线后结束当前执行与实例，不重启轮次，也不改写提交版本。
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
    ensure_reject_transition(graph, reject_edge)?;

    current.record_reject(command.actor.clone(), reason, command.now)?;
    instance.complete_rejected(command.now)?;
    Ok(completed_current_plan(instance, current, command.actor, BpmEventKind::NodeRejected))
}

// 接受决定后把已结束的执行、对应节点事件和任务完成意图放入同一计划，供通过与驳回共用。
fn completed_current_plan(
    instance: ApprovalProcessInstance,
    current: ApprovalNodeExecution,
    actor: ParticipantId,
    kind: BpmEventKind,
) -> TransitionPlan {
    let execution_id = current.typed_id();
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Proceed);
    plan.events.push(
        BpmEvent::new(kind, plan.instance.typed_id(), current.round_no)
            .with_execution(execution_id.clone())
            .with_node_key(current.node_key.clone())
            .with_actor(actor),
    );
    plan.task_intents.push(TaskIntent::CompleteTask { execution_id });
    plan.updated_executions.push(current);
    plan
}

// 非末节点通过后复用进入规则创建同轮执行；即使下一节点受阻，也保留已接受决定所需的 `Proceed` 提交类别。
fn enter_after_decision(
    mut plan: TransitionPlan,
    graph: &DefinitionGraph,
    node_key: &str,
    command: DecideCommand,
) -> EngineResult<TransitionPlan> {
    let enter = plan_enter_node(EnterNodeInput {
        instance: plan.instance.clone(),
        graph,
        node_key,
        round_no: plan.instance.current_round_no,
        participant: command.next_eligibility.participant(),
        eligibility: command.next_eligibility,
        execution_id: command.next_execution_id,
        execution_no: command.next_execution_no,
        assignment_source: ApprovalExecutionAssignmentSource::Definition,
        replaces_execution_id: None,
        now: command.now,
    })?;
    plan.merge_enter_keep_commit(enter);
    Ok(plan)
}

/// 发现当前活动执行无法继续时，生成一次完整的阻塞迁移：更新实例和执行、关闭关联任务并发出 `InstanceBlocked`。
/// 调用方须一并提交这些事实，不能只写实例的阻塞投影。
///
/// # 错误
/// 终态、非当前令牌、非活动执行或模型拒绝阻塞迁移时，不生成可提交计划。
pub fn block_current(
    mut instance: ApprovalProcessInstance,
    mut current: ApprovalNodeExecution,
    code: ApprovalBlockerCode,
    now: Timestamp,
) -> EngineResult<TransitionPlan> {
    ensure_current_token(&instance, &current)?;
    current.block(code, now)?;
    instance.enter_blocked(code, now)?;

    let execution_id = current.typed_id();
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Blocked);
    plan.events.push(
        BpmEvent::new(BpmEventKind::InstanceBlocked, plan.instance.typed_id(), current.round_no)
            .with_execution(execution_id.clone())
            .with_node_key(current.node_key.clone())
            .with_blocker(code),
    );
    plan.task_intents
        .push(TaskIntent::CloseTask { execution_id, reason: TaskCloseReason::ApprovalRuntimeBlocked });
    plan.updated_executions.push(current);
    Ok(plan)
}

// 接受决定或记录阻塞前，确认实例未终结、执行仍是其当前活动令牌，避免更新已被替换或结束的执行。
fn ensure_current_token(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
) -> EngineResult<()> {
    if instance.status.is_terminal() {
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

// 驳回前确认连线表示终止；旧定义指回入口的驳回边也按终止处理，不再用它重开流程。
fn ensure_reject_transition(
    graph: &DefinitionGraph,
    reject_edge: &ApprovalTransitionDefinition,
) -> EngineResult<()> {
    if reject_edge.event != ApprovalTransitionEvent::Reject {
        return Err(EngineError::GraphCorrupted);
    }
    match (reject_edge.to_node_key.as_deref(), reject_edge.terminal_result) {
        (None, Some(ApprovalTerminalResult::Rejected)) => Ok(()),
        (Some(key), None) if key == graph.definition.entry_node_key => Ok(()),
        _ => Err(EngineError::GraphCorrupted),
    }
}
