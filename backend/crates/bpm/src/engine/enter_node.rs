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

/// 规划进入指定节点。
///
/// 资格有效时创建 `ACTIVE` 执行和中性任务请求；资格无效时创建 `BLOCKED`
/// 执行且不产生任务。图缺少节点时，若无法形成合法快照则返回不可提交错误。
///
/// # 参数
/// * `instance` - 当前实例快照
/// * `graph` - 已加载的定义图
/// * `node_key` - 目标节点
/// * `round_no` - 本次进入所属轮次
/// * `participant` - 当前责任人
/// * `eligibility` - 调用方已收敛的资格结果
/// * `execution_id` - 调用方提供的新执行 ID
/// * `execution_no` - 实例内单调递增序号
/// * `assignment_source` - 本次进入来源
/// * `replaces_execution_id` - 被替换的旧执行
/// * `now` - 调用方时间
///
/// # 错误
/// 节点缺失且无法形成合法快照，或模型不变式失败时返回错误。
#[allow(clippy::too_many_arguments)]
pub fn plan_enter_node(
    instance: ApprovalProcessInstance,
    graph: &DefinitionGraph,
    node_key: &str,
    round_no: u32,
    participant: ParticipantId,
    eligibility: Eligibility,
    execution_id: ApprovalNodeExecutionId,
    execution_no: u32,
    assignment_source: ApprovalExecutionAssignmentSource,
    replaces_execution_id: Option<ApprovalNodeExecutionId>,
    now: Timestamp,
) -> EngineResult<TransitionPlan> {
    let node = match graph.node(node_key) {
        Some(node) => node,
        None => {
            return structural_enter_without_node(instance, node_key, participant, execution_id, now);
        }
    };
    build_enter_plan(
        instance,
        node,
        round_no,
        participant,
        eligibility,
        execution_id,
        execution_no,
        assignment_source,
        replaces_execution_id,
        now,
    )
}

/// 在已解析节点上构造进入计划。
#[allow(clippy::too_many_arguments)]
fn build_enter_plan(
    mut instance: ApprovalProcessInstance,
    node: &ApprovalNodeDefinition,
    round_no: u32,
    participant: ParticipantId,
    eligibility: Eligibility,
    execution_id: ApprovalNodeExecutionId,
    execution_no: u32,
    assignment_source: ApprovalExecutionAssignmentSource,
    replaces_execution_id: Option<ApprovalNodeExecutionId>,
    now: Timestamp,
) -> EngineResult<TransitionPlan> {
    let input = NewNodeExecution {
        id: execution_id.clone(),
        process_instance_id: crate::ids::ApprovalProcessInstanceId::new(instance.base.id.clone()),
        node_key: node.node_key.clone(),
        node_name: node.node_name.clone(),
        round_no,
        execution_no,
        assignment_source,
        replaces_execution_id,
        assignee_participant_id: participant.clone(),
        assignee_name_snapshot: eligibility.name_snapshot().to_string(),
        at: now,
    };
    if let Some(code) = eligibility.blocked_code() {
        return blocked_enter(instance, input, execution_id, node, code, now);
    }
    let execution = ApprovalNodeExecution::new_active(input)?;
    instance.set_current_execution(execution_id.clone(), now)?;
    if instance.status == ApprovalProcessInstanceStatus::Blocked {
        instance.exit_blocked(now)?;
    }
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Proceed);
    let entered = BpmEvent::new(
        BpmEventKind::NodeEntered,
        crate::ids::ApprovalProcessInstanceId::new(plan.instance.base.id.clone()),
        round_no,
    )
    .with_execution(execution_id.clone())
    .with_node_key(node.node_key.clone())
    .with_actor(participant.clone());
    plan.events.push(entered);
    plan.task_intents.push(TaskIntent::HumanTaskRequested {
        execution_id,
        assignee: participant,
        node_key: node.node_key.clone(),
        node_name: node.node_name.clone(),
        round_no,
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
