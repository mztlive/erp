//! 原审批人恢复：结束旧受阻执行并在同轮同节点创建新活动执行。

use crate::ids::ApprovalNodeExecutionId;
use crate::model::types::{
    ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason, ApprovalNodeExecutionStatus,
    ApprovalProcessInstanceStatus,
};
use crate::model::{ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance, Timestamp};

use super::enter_node::plan_enter_node;
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
/// 非人员失效阻塞、绑定已改派或资格仍失效时返回错误。
pub fn resume(
    instance: ApprovalProcessInstance,
    mut current: ApprovalNodeExecution,
    assignee: &ApprovalInstanceAssignee,
    graph: &DefinitionGraph,
    command: ResumeCommand,
) -> EngineResult<TransitionPlan> {
    ensure_personnel_blocked(&instance, &current)?;
    if assignee.assignment_source != crate::model::types::ApprovalAssigneeBindingSource::Definition {
        return Err(EngineError::InvalidCommand(
            "实例审批人已被改派，不得走原审批人恢复",
        ));
    }
    if assignee.current_assignee_participant_id != current.assignee_participant_id {
        return Err(EngineError::InvalidCommand("当前审批人与旧受阻执行不一致"));
    }
    if command.eligibility.blocked_code().is_some() {
        return Err(EngineError::InvalidCommand("原审批人资格尚未恢复"));
    }
    current.supersede(ApprovalExecutionEndReason::AssigneeRecovered, command.now)?;
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
    let enter = plan_enter_node(
        plan.instance.clone(),
        graph,
        &current.node_key,
        current.round_no,
        current.assignee_participant_id.clone(),
        command.eligibility,
        command.next_execution_id,
        command.next_execution_no,
        ApprovalExecutionAssignmentSource::AssigneeRecovery,
        Some(ApprovalNodeExecutionId::new(current.base.id.clone())),
        command.now,
    )?;
    plan.updated_executions.push(current);
    plan.merge_enter(enter, false);
    plan.events.push(BpmEvent::new(
        BpmEventKind::AssigneeRecovered,
        crate::ids::ApprovalProcessInstanceId::new(plan.instance.base.id.clone()),
        plan.instance.current_round_no,
    ));
    Ok(plan)
}

/// 只允许人员失效类别的受阻实例与受阻执行进入恢复。
fn ensure_personnel_blocked(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
) -> EngineResult<()> {
    if instance.status != ApprovalProcessInstanceStatus::Blocked
        || current.status != ApprovalNodeExecutionStatus::Blocked
    {
        return Err(EngineError::InvalidCommand("只有受阻实例和受阻执行可以恢复"));
    }
    instance.ensure_personnel_reassign_allowed()?;
    let Some(code) = current.blocker_code else {
        return Err(EngineError::InvalidCommand("受阻执行缺少 blocker"));
    };
    if !code.allows_personnel_reassign() {
        return Err(EngineError::InvalidCommand("结构性阻塞不得恢复原审批人"));
    }
    Ok(())
}
