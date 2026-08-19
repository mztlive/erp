//! 管理员改派：更新实例绑定并替换受阻执行。

use crate::ids::ApprovalNodeExecutionId;
use crate::model::types::{
    ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason, ApprovalNodeExecutionStatus,
    ApprovalProcessInstanceStatus,
};
use crate::model::{
    ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId, Timestamp,
};

use super::enter_node::plan_enter_node;
use super::event::{BpmEvent, BpmEventKind};
use super::transition_plan::{CommitRequired, TransitionPlan};
use super::{DefinitionGraph, Eligibility, EngineError, EngineResult};

/// 改派命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReassignCommand {
    /// 目标审批人。
    pub target: ParticipantId,
    /// 改派人。
    pub actor: ParticipantId,
    /// 非空改派原因。
    pub reason: String,
    /// 目标用户资格。
    pub target_eligibility: Eligibility,
    /// 新执行主键。
    pub next_execution_id: ApprovalNodeExecutionId,
    /// 新执行序号。
    pub next_execution_no: u32,
    /// 调用方时间。
    pub now: Timestamp,
}

/// 更新实例节点当前审批人，结束旧受阻执行，并创建 `ADMIN_REASSIGN` 新执行。
///
/// 定义审批人快照保持不变。正常 `ACTIVE` 节点不得改派。
///
/// # 错误
/// 非人员失效阻塞、目标非法或资格无效时返回错误。
pub fn reassign(
    instance: ApprovalProcessInstance,
    mut current: ApprovalNodeExecution,
    mut assignee: ApprovalInstanceAssignee,
    graph: &DefinitionGraph,
    command: ReassignCommand,
) -> EngineResult<TransitionPlan> {
    ensure_reassignable(&instance, &current)?;
    let reason = command.reason.trim();
    if reason.is_empty() {
        return Err(EngineError::InvalidCommand("改派原因不能为空"));
    }
    if command.target_eligibility.blocked_code().is_some() {
        return Err(EngineError::InvalidCommand("目标审批人资格无效"));
    }
    assignee.reassign(command.target.clone(), command.actor.clone(), reason, command.now)?;
    current.supersede(ApprovalExecutionEndReason::AdminReassigned, command.now)?;
    let mut plan = TransitionPlan::for_instance(instance, CommitRequired::Proceed);
    plan.updated_assignees.push(assignee);
    plan.events.push(
        BpmEvent::new(
            BpmEventKind::ExecutionSuperseded,
            crate::ids::ApprovalProcessInstanceId::new(plan.instance.base.id.clone()),
            current.round_no,
        )
        .with_execution(ApprovalNodeExecutionId::new(current.base.id.clone()))
        .with_node_key(current.node_key.clone()),
    );
    let enter = plan_enter_node(super::EnterNodeInput {
        instance: plan.instance.clone(),
        graph,
        node_key: &current.node_key,
        round_no: current.round_no,
        participant: command.target.clone(),
        eligibility: command.target_eligibility,
        execution_id: command.next_execution_id,
        execution_no: command.next_execution_no,
        assignment_source: ApprovalExecutionAssignmentSource::AdminReassign,
        replaces_execution_id: Some(ApprovalNodeExecutionId::new(current.base.id.clone())),
        now: command.now,
    })?;
    plan.updated_executions.push(current);
    plan.merge_enter(enter, false);
    plan.events.push(
        BpmEvent::new(
            BpmEventKind::AssigneeReassigned,
            crate::ids::ApprovalProcessInstanceId::new(plan.instance.base.id.clone()),
            plan.instance.current_round_no,
        )
        .with_actor(command.actor)
        .with_reason(reason),
    );
    Ok(plan)
}

/// 只允许人员失效类别的受阻实例改派。
fn ensure_reassignable(
    instance: &ApprovalProcessInstance,
    current: &ApprovalNodeExecution,
) -> EngineResult<()> {
    if instance.status != ApprovalProcessInstanceStatus::Blocked
        || current.status != ApprovalNodeExecutionStatus::Blocked
    {
        return Err(EngineError::InvalidCommand("只有受阻实例和受阻执行可以改派"));
    }
    instance.ensure_personnel_reassign_allowed()?;
    Ok(())
}
