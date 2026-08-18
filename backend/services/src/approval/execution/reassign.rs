//! 管理员改派编排。

use bpm::engine::{reassign, ReassignCommand};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId};
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{
    ApprovalCommandReceipt, ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance,
    ParticipantId,
};

use super::apply_plan::apply_plan;
use super::authorization::is_personnel_blocker;
use super::idempotency::{classify_receipt, reassign_digest, ReceiptBranch};
use super::start::map_engine_error;
use super::{ExecutionCommandInput, PreparedExecution};
use crate::errors::{Error, Result};

/// 改派编排输入。
#[derive(Debug, Clone)]
pub struct ReassignExecutionInput {
    /// 公共输入。
    pub command: ExecutionCommandInput,
    /// 当前实例。
    pub instance: ApprovalProcessInstance,
    /// 当前受阻执行。
    pub current: ApprovalNodeExecution,
    /// 实例审批人绑定。
    pub assignee: ApprovalInstanceAssignee,
    /// 目标用户。
    pub target: ParticipantId,
    /// 改派人。
    pub actor: ParticipantId,
    /// 非空原因。
    pub reason: String,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 期望绑定版本。
    pub expected_assignment_version: u64,
    /// 可空任务版本。
    pub expected_task_version: Option<u64>,
    /// 新执行主键。
    pub next_execution_id: ApprovalNodeExecutionId,
    /// 新执行序号。
    pub next_execution_no: u32,
    /// 收据主键。
    pub receipt_id: ApprovalCommandReceiptId,
}

/// 规划管理员改派。原审批人已恢复时不得改派。
///
/// # 参数
/// * `input` - 改派输入
///
/// # 错误
/// 非人员失效、原审批人已恢复、异载荷冲突或引擎失败时返回错误。
pub fn prepare_reassign(input: ReassignExecutionInput) -> Result<PreparedExecution> {
    let Some(code) = input.current.blocker_code else {
        return Err(Error::ValidationError(
            "改派要求当前执行为人员失效阻塞".to_string(),
        ));
    };
    if !is_personnel_blocker(code) {
        return Err(Error::ValidationError("结构性阻塞不得改派".to_string()));
    }
    if input.command.current_eligibility.blocked_code().is_none() {
        return Err(Error::ConflictError(
            "APPROVAL_CURRENT_APPROVER_RECOVERED".to_string(),
        ));
    }
    let digest = reassign_digest(
        input.target.as_str(),
        input.expected_instance_version,
        input.expected_execution_version,
        input.expected_assignment_version,
        input.expected_task_version,
        &input.reason,
        input.actor.as_str(),
    );
    match classify_receipt(input.command.receipt.as_ref(), &digest) {
        ReceiptBranch::PayloadConflict => return Err(super::idempotency::payload_conflict_error()),
        ReceiptBranch::SamePayload(receipt) => {
            return Ok(PreparedExecution::Replay {
                receipt: receipt.clone(),
            });
        }
        ReceiptBranch::Fresh => {}
    }
    let plan = reassign(
        input.instance,
        input.current,
        input.assignee,
        &input.command.graph,
        ReassignCommand {
            target: input.target,
            actor: input.actor,
            reason: input.reason,
            target_eligibility: input.command.next_eligibility,
            next_execution_id: input.next_execution_id,
            next_execution_no: input.next_execution_no,
            now: input.command.now,
        },
    )
    .map_err(map_engine_error)?;
    let receipt = ApprovalCommandReceipt::new(
        input.receipt_id,
        ApprovalCommandKind::ReassignApprover,
        plan.created_executions
            .first()
            .map(|item| item.base.id.clone())
            .unwrap_or_else(|| plan.instance.base.id.clone()),
        input.command.idempotency_key,
        digest,
        plan.instance.base.id.clone(),
        input.command.now,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(PreparedExecution::Apply(Box::new(apply_plan(
        plan, receipt, None,
    ))))
}
