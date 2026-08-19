//! 统一节点进入。启动、通过、驳回、恢复与改派必须复用本函数。

use crate::ids::ApprovalNodeExecutionId;
use crate::model::types::{
    ApprovalBlockerCode, ApprovalExecutionAssignmentSource, ApprovalProcessInstanceStatus,
    ApprovalTransitionEvent,
};
use crate::model::{
    ApprovalNodeDefinition, ApprovalNodeExecution, ApprovalProcessInstance, ApprovalTransitionDefinition,
    NewNodeExecution, ParticipantId, Timestamp,
};

use super::event::{BpmEvent, BpmEventKind};
use super::transition_plan::{CommitRequired, TaskIntent, TransitionPlan};
use super::{DefinitionGraph, Eligibility, EngineError, EngineResult};

/// 规划进入指定节点所需的实例、图与责任快照。
pub struct EnterNodeInput<'a> {
    /// 当前实例快照。
    pub instance: ApprovalProcessInstance,
    /// 已加载的定义图。
    pub graph: &'a DefinitionGraph,
    /// 目标节点键。
    pub node_key: &'a str,
    /// 本次进入所属轮次。
    pub round_no: u32,
    /// 当前责任人。
    pub participant: ParticipantId,
    /// 调用方已收敛的资格结果。
    pub eligibility: Eligibility,
    /// 调用方提供的新执行 ID。
    pub execution_id: ApprovalNodeExecutionId,
    /// 实例内单调递增序号。
    pub execution_no: u32,
    /// 本次进入来源。
    pub assignment_source: ApprovalExecutionAssignmentSource,
    /// 被替换的旧执行。
    pub replaces_execution_id: Option<ApprovalNodeExecutionId>,
    /// 调用方时间。
    pub now: Timestamp,
}

/// 规划进入指定节点。
///
/// 资格有效时创建 `ACTIVE` 执行和中性任务请求；资格无效时创建 `BLOCKED`
/// 执行且不产生任务。图缺少节点时，若无法形成合法快照则返回不可提交错误。
///
/// # 参数
/// * `input` - 实例、目标节点与责任快照
///
/// # 返回
/// 返回可提交的进入计划。
///
/// # 错误
/// 节点缺失且无法形成合法快照，或模型不变式失败时返回错误。
///
/// # 约束
/// 启动、通过、驳回、恢复与改派必须复用本函数。
pub fn plan_enter_node(input: EnterNodeInput<'_>) -> EngineResult<TransitionPlan> {
    let node = match input.graph.node(input.node_key) {
        Some(node) => node,
        None => {
            return structural_enter_without_node(
                input.instance,
                input.node_key,
                input.participant,
                input.execution_id,
                input.now,
            );
        }
    };
    build_enter_plan(input, node)
}

/// 在已解析节点上构造进入计划。
///
/// # 参数
/// * `input` - 与 [`plan_enter_node`] 相同的进入快照
/// * `node` - 已从图中解析的目标节点
///
/// # 返回
/// 返回进入计划。
///
/// # 错误
/// 模型不变式失败时返回错误。
///
/// # 约束
/// 不得绕过 [`plan_enter_node`] 直接构造进入事实。
fn build_enter_plan(
    mut input: EnterNodeInput<'_>,
    node: &ApprovalNodeDefinition,
) -> EngineResult<TransitionPlan> {
    let execution_input = NewNodeExecution {
        id: input.execution_id.clone(),
        process_instance_id: crate::ids::ApprovalProcessInstanceId::new(input.instance.base.id.clone()),
        node_key: node.node_key.clone(),
        node_name: node.node_name.clone(),
        round_no: input.round_no,
        execution_no: input.execution_no,
        assignment_source: input.assignment_source,
        replaces_execution_id: input.replaces_execution_id,
        assignee_participant_id: input.participant.clone(),
        assignee_name_snapshot: input.eligibility.name_snapshot().to_string(),
        at: input.now,
    };
    if let Some(code) = input.eligibility.blocked_code() {
        return blocked_enter(
            input.instance,
            execution_input,
            input.execution_id,
            node,
            code,
            input.now,
        );
    }
    let execution = ApprovalNodeExecution::new_active(execution_input)?;
    input
        .instance
        .set_current_execution(input.execution_id.clone(), input.now)?;
    if input.instance.status == ApprovalProcessInstanceStatus::Blocked {
        input.instance.exit_blocked(input.now)?;
    }
    let mut plan = TransitionPlan::for_instance(input.instance, CommitRequired::Proceed);
    let entered = BpmEvent::new(
        BpmEventKind::NodeEntered,
        crate::ids::ApprovalProcessInstanceId::new(plan.instance.base.id.clone()),
        input.round_no,
    )
    .with_execution(input.execution_id.clone())
    .with_node_key(node.node_key.clone())
    .with_actor(input.participant.clone());
    plan.events.push(entered);
    plan.task_intents.push(TaskIntent::HumanTaskRequested {
        execution_id: input.execution_id,
        assignee: input.participant,
        node_key: node.node_key.clone(),
        node_name: node.node_name.clone(),
        round_no: input.round_no,
    });
    plan.created_executions.push(execution);
    Ok(plan)
}

/// 人员或结构资格无效时创建受阻执行。
fn blocked_enter(
    mut instance: ApprovalProcessInstance,
    input: NewNodeExecution,
    execution_id: ApprovalNodeExecutionId,
    node: &ApprovalNodeDefinition,
    code: ApprovalBlockerCode,
    now: Timestamp,
) -> EngineResult<TransitionPlan> {
    let execution = ApprovalNodeExecution::new_blocked(input, code)?;
    instance.set_current_execution(execution_id.clone(), now)?;
    instance.enter_blocked(code, now)?;
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Blocked);
    plan.events.push(
        BpmEvent::new(
            BpmEventKind::InstanceBlocked,
            crate::ids::ApprovalProcessInstanceId::new(plan.instance.base.id.clone()),
            node_round(&execution),
        )
        .with_execution(execution_id)
        .with_node_key(node.node_key.clone())
        .with_blocker(code),
    );
    plan.created_executions.push(execution);
    Ok(plan)
}

/// 节点定义缺失时，若缺少构造执行所需字段则拒绝提交半结构实体。
fn structural_enter_without_node(
    _instance: ApprovalProcessInstance,
    node_key: &str,
    _participant: ParticipantId,
    _execution_id: ApprovalNodeExecutionId,
    _now: Timestamp,
) -> EngineResult<TransitionPlan> {
    if node_key.trim().is_empty() {
        return Err(EngineError::Uncommittable("缺失节点键，无法形成合法阻塞快照"));
    }
    Err(EngineError::Uncommittable(
        "定义图缺少目标节点，无法形成合法阻塞快照",
    ))
}

/// 读取当前节点的通过与驳回连线，必须各恰好一条。
///
/// # 错误
/// 缺失或重复时返回图损坏。
pub(crate) fn require_decision_edges<'a>(
    graph: &'a DefinitionGraph,
    from_node_key: &str,
) -> EngineResult<(&'a ApprovalTransitionDefinition, &'a ApprovalTransitionDefinition)> {
    let approve = unique_transition(graph, from_node_key, ApprovalTransitionEvent::Approve)?;
    let reject = unique_transition(graph, from_node_key, ApprovalTransitionEvent::Reject)?;
    Ok((approve, reject))
}

/// 读取指定事件的唯一连线。
pub(crate) fn unique_transition<'a>(
    graph: &'a DefinitionGraph,
    from_node_key: &str,
    event: ApprovalTransitionEvent,
) -> EngineResult<&'a ApprovalTransitionDefinition> {
    let matches: Vec<_> = graph
        .transitions
        .iter()
        .filter(|item| item.from_node_key == from_node_key && item.event == event)
        .collect();
    match matches.as_slice() {
        [only] => {
            only.validate_shape()?;
            Ok(*only)
        }
        _ => Err(EngineError::GraphCorrupted),
    }
}

/// 从执行读取轮次。
fn node_round(execution: &ApprovalNodeExecution) -> u32 {
    execution.round_no
}
