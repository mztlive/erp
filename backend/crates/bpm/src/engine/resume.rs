//! 原审批人恢复：结束旧受阻执行并在同轮同节点创建新活动执行。

use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use crate::model::types::{
    ApprovalExecutionAssignmentSource, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
};
use crate::model::{ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance, Timestamp};

use super::enter_node::{plan_enter_node, EnterNodeInput};
use super::event::{BpmEvent, BpmEventKind};
use super::transition_plan::{CommitRequired, TransitionPlan};
use super::{DefinitionGraph, Eligibility, EngineError, EngineResult};

/// 恢复命令。不接受目标用户或节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeCommand {
    /// 新执行主键。
    pub next_execution_id: ApprovalNodeExecutionId,
    /// 新执行序号，必须大于旧执行。
    pub next_execution_no: u32,
    /// 原审批人已恢复后的资格。
    pub eligibility: Eligibility,
    /// 调用方时间。
    pub now: Timestamp,
}

/// 将旧 `BLOCKED` 执行置为 `SUPERSEDED`，并以 `ASSIGNEE_RECOVERY` 进入同一节点。
///
/// # 错误
/// 非人员失效阻塞、绑定不再等于定义快照或原审批人资格仍失效时返回错误。
pub fn resume(
    instance: ApprovalProcessInstance,
    mut current: ApprovalNodeExecution,
    assignee: &ApprovalInstanceAssignee,
    graph: &DefinitionGraph,
    command: ResumeCommand,
) -> EngineResult<TransitionPlan> {
    ensure_assignee_recovery_state(&instance, &current, assignee, graph, &command.eligibility)?;
    if command.next_execution_no <= current.execution_no {
        return Err(EngineError::InvalidCommand("恢复执行序号必须递增"));
    }
    if command.next_execution_id.as_ref() == current.base.id.as_str() {
        return Err(EngineError::InvalidCommand("恢复执行必须使用新主键"));
    }
    if command.eligibility.blocked_code().is_some() {
        return Err(EngineError::InvalidCommand("原审批人资格尚未恢复"));
    }
    let recovered_execution_id = command.next_execution_id.clone();
    current.supersede_for_assignee_recovery(command.now)?;
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Proceed);
    plan.events.push(
        BpmEvent::new(
            BpmEventKind::ExecutionSuperseded,
            crate::ids::ApprovalProcessInstanceId::new(plan.instance.base.id.clone()),
            current.round_no,
        )
        .with_execution(ApprovalNodeExecutionId::new(current.base.id.clone()))
        .with_node_key(current.node_key.clone()),
    );
    let enter = plan_enter_node(EnterNodeInput {
        instance: plan.instance.clone(),
        graph,
        node_key: &current.node_key,
        round_no: current.round_no,
        participant: assignee.definition_assignee_participant_id.clone(),
        eligibility: command.eligibility,
        execution_id: command.next_execution_id,
        execution_no: command.next_execution_no,
        assignment_source: ApprovalExecutionAssignmentSource::AssigneeRecovery,
        replaces_execution_id: Some(ApprovalNodeExecutionId::new(current.base.id.clone())),
        now: command.now,
    })?;
    plan.updated_executions.push(current);
    plan.merge_enter(enter, false);
    plan.events.push(
        BpmEvent::new(
            BpmEventKind::AssigneeRecovered,
            crate::ids::ApprovalProcessInstanceId::new(plan.instance.base.id.clone()),
            plan.instance.current_round_no,
        )
        .with_execution(recovered_execution_id),
    );
    Ok(plan)
}

/// 恢复前校验实例、执行、定义绑定和资格属于同一原审批人。
fn ensure_assignee_recovery_state(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
    assignee: &ApprovalInstanceAssignee,
    graph: &DefinitionGraph,
    eligibility: &Eligibility,
) -> EngineResult<()> {
    if instance.status != ApprovalProcessInstanceStatus::Blocked
        || current.status != ApprovalNodeExecutionStatus::Blocked
    {
        return Err(EngineError::InvalidCommand("只有受阻实例和受阻执行可以恢复"));
    }
    instance.ensure_assignee_recovery_allowed()?;
    let Some(code) = current.blocker_code else {
        return Err(EngineError::InvalidCommand("受阻执行缺少 blocker"));
    };
    if !code.allows_assignee_recovery() {
        return Err(EngineError::InvalidCommand("结构性阻塞不得恢复原审批人"));
    }
    if instance.blocker_code != current.blocker_code {
        return Err(EngineError::InvalidCommand("实例与执行 blocker 不一致"));
    }

    let instance_id = ApprovalProcessInstanceId::new(instance.base.id.clone());
    if current.process_instance_id != instance_id || assignee.process_instance_id != instance_id {
        return Err(EngineError::InvalidCommand("恢复事实不属于同一审批实例"));
    }
    let current_execution_id = ApprovalNodeExecutionId::new(current.base.id.clone());
    if instance.current_node_execution_id.as_ref() != Some(&current_execution_id) {
        return Err(EngineError::InvalidCommand("执行不是实例当前令牌"));
    }
    if current.round_no != instance.current_round_no {
        return Err(EngineError::InvalidCommand("受阻执行不属于实例当前轮次"));
    }
    if graph.definition.base.id != instance.process_definition_id.as_ref()
        || graph.definition.definition_version != instance.definition_version
    {
        return Err(EngineError::InvalidCommand("恢复图与实例冻结定义不一致"));
    }

    assignee.ensure_unchanged_from_definition()?;
    if assignee.node_key != current.node_key {
        return Err(EngineError::InvalidCommand("实例审批人绑定与受阻节点不一致"));
    }
    if assignee.definition_assignee_participant_id != current.assignee_participant_id {
        return Err(EngineError::InvalidCommand("原审批人与旧受阻执行不一致"));
    }
    if eligibility.participant() != assignee.definition_assignee_participant_id {
        return Err(EngineError::InvalidCommand("资格结果必须属于原审批人"));
    }
    let node = graph
        .node(&assignee.node_key)
        .ok_or(EngineError::GraphCorrupted)?;
    if node.assignee_participant_id != assignee.definition_assignee_participant_id {
        return Err(EngineError::GraphCorrupted);
    }
    Ok(())
}
