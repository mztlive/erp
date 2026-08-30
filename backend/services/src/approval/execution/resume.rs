//! 恢复原审批人编排。

use bpm::engine::{resume, ResumeCommand};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId};
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{
    ApprovalCommandReceipt, ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance,
};

use super::apply_plan::apply_plan;
use super::authorization::is_personnel_blocker;
use super::idempotency::{classify_receipt, resume_digest, ReceiptBranch};
use super::start::map_engine_error;
use super::{ExecutionCommandInput, PreparedExecution};
use crate::errors::{Error, Result};

/// 恢复编排输入。
#[derive(Debug, Clone)]
pub struct ResumeExecutionInput {
    /// 公共输入。
    pub command: ExecutionCommandInput,
    /// 当前实例。
    pub instance: ApprovalProcessInstance,
    /// 当前受阻执行。
    pub current: ApprovalNodeExecution,
    /// 实例审批人绑定。
    pub assignee: ApprovalInstanceAssignee,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 期望绑定版本。
    pub expected_assignment_version: u64,
    /// 可空已关闭任务版本。
    pub expected_closed_task_version: Option<u64>,
    /// 新执行主键。
    pub next_execution_id: ApprovalNodeExecutionId,
    /// 新执行序号。
    pub next_execution_no: u32,
    /// 收据主键。
    pub receipt_id: ApprovalCommandReceiptId,
    /// 恢复人。
    pub actor_id: String,
}

/// 规划原审批人恢复。
///
/// # 参数
/// * `input` - 恢复输入
///
/// # 错误
/// 非人员失效、异载荷冲突或引擎失败时返回错误。
pub fn prepare_resume(input: ResumeExecutionInput) -> Result<PreparedExecution> {
    let digest = resume_digest(
        input.expected_instance_version,
        input.expected_execution_version,
        input.expected_assignment_version,
        input.expected_closed_task_version,
        &input.actor_id,
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
    let Some(code) = input.current.blocker_code else {
        return Err(Error::ValidationError(
            "恢复要求当前执行为人员失效阻塞".to_string(),
        ));
    };
    if !is_personnel_blocker(code) {
        return Err(Error::ValidationError("结构性阻塞不得恢复原审批人".to_string()));
    }
    let receipt_scope = input.instance.base.id.clone();
    let plan = resume(
        input.instance,
        input.current,
        &input.assignee,
        &input.command.graph,
        ResumeCommand {
            next_execution_id: input.next_execution_id,
            next_execution_no: input.next_execution_no,
            eligibility: input.command.current_eligibility,
            now: input.command.now,
        },
    )
    .map_err(map_engine_error)?;
    let receipt = ApprovalCommandReceipt::new(
        input.receipt_id,
        ApprovalCommandKind::ResumeApprover,
        receipt_scope,
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
