//! 原审批人恢复：结束旧受阻执行并在同轮同节点创建新活动执行。

use super::enter_node::{EnterNodeInput, plan_enter_node};
use super::event::{BpmEvent, BpmEventKind};
use super::transition_plan::{CommitRequired, TransitionPlan};
use super::{DefinitionGraph, Eligibility, EngineError, EngineResult};
use crate::ids::ApprovalNodeExecutionId;
use crate::model::types::{
    ApprovalExecutionAssignmentSource, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
};
use crate::model::{ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance, Timestamp};

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

/// 原审批人资格恢复后结束旧受阻执行，并以 `ASSIGNEE_RECOVERY` 在同一轮、同一节点创建活动执行和任务意图。
/// 恢复沿用实例冻结的定义与审批人绑定，不允许换人、换节点或重新打开旧执行。
///
/// # 错误
/// 非人员失效阻塞、实例与执行或冻结归属不符、资格仍失效、新 ID 与旧 ID 相同、新序号未递增或无法进入节点时返回错误。
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
        BpmEvent::new(BpmEventKind::ExecutionSuperseded, plan.instance.typed_id(), current.round_no)
            .with_execution(current.typed_id())
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
        replaces_execution_id: Some(current.typed_id()),
        now: command.now,
    })?;
    plan.updated_executions.push(current);
    plan.merge_enter_adopt_commit(enter);

    plan.events.push(
        BpmEvent::new(
            BpmEventKind::AssigneeRecovered,
            plan.instance.typed_id(),
            plan.instance.current_round_no,
        )
        .with_execution(recovered_execution_id),
    );
    Ok(plan)
}

// 替换旧执行前，依次核对阻塞、实例与执行归属、冻结人员与资格，确保恢复只作用于原人原节点。
fn ensure_assignee_recovery_state(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
    assignee: &ApprovalInstanceAssignee,
    graph: &DefinitionGraph,
    eligibility: &Eligibility,
) -> EngineResult<()> {
    ensure_blocker_allows_recovery(instance, current)?;
    ensure_instance_execution_affinity(instance, current, assignee, graph)?;
    ensure_binding_eligibility_affinity(current, assignee, graph, eligibility)?;
    Ok(())
}

// 先确认实例和执行都处于可原人恢复的受阻态，且阻塞码一致；结构或投影不一致不能靠重派任务解决。
fn ensure_blocker_allows_recovery(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
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
    Ok(())
}

// 恢复前核对绑定和旧执行都属于当前实例，并确认令牌、轮次和冻结定义版本仍匹配，避免恢复过期执行。
fn ensure_instance_execution_affinity(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
    assignee: &ApprovalInstanceAssignee,
    graph: &DefinitionGraph,
) -> EngineResult<()> {
    let instance_id = instance.typed_id();
    if current.process_instance_id != instance_id || assignee.process_instance_id != instance_id {
        return Err(EngineError::InvalidCommand("恢复事实不属于同一审批实例"));
    }

    let current_execution_id = current.typed_id();
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
    Ok(())
}

// 用冻结绑定校验节点、旧执行及资格都指向原审批人和同一或签名单，不让恢复变成改派。
fn ensure_binding_eligibility_affinity(
    current: &ApprovalNodeExecution,
    assignee: &ApprovalInstanceAssignee,
    graph: &DefinitionGraph,
    eligibility: &Eligibility,
) -> EngineResult<()> {
    assignee.ensure_unchanged_from_definition()?;
    if assignee.node_key != current.node_key {
        return Err(EngineError::InvalidCommand("实例审批人绑定与受阻节点不一致"));
    }
    if assignee.definition_assignee_participant_id != current.assignee_participant_id {
        return Err(EngineError::InvalidCommand("原审批人与旧受阻执行不一致"));
    }
    if eligibility.participant_ref() != &assignee.definition_assignee_participant_id {
        return Err(EngineError::InvalidCommand("资格结果必须属于原审批人"));
    }

    let node = graph.node(&assignee.node_key).ok_or(EngineError::GraphCorrupted)?;
    if node.additional_assignees != assignee.additional_assignees
        || current.additional_assignees != assignee.additional_assignees
        || *node.assignee()? != assignee.definition_assignee_participant_id
    {
        return Err(EngineError::GraphCorrupted);
    }
    Ok(())
}
