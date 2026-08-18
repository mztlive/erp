//! 决定编排核心：通过、驳回与当前责任人失效阻塞。

use bpm::engine::{decide, CommitRequired, DecideCommand, TransitionPlan};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId};
use bpm::model::types::{ApprovalCommandKind, ApprovalDecision};
use bpm::model::{ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId};

use super::apply_plan::{apply_plan, DomainActionKind};
use super::authorization::ensure_triple_responsibility;
use super::idempotency::{classify_receipt, decision_digest, ReceiptBranch};
use super::start::map_engine_error;
use super::{ExecutionCommandInput, PreparedExecution};
use crate::errors::{Error, Result};

/// 决定编排输入。
#[derive(Debug, Clone)]
pub struct DecisionExecutionInput {
    /// 公共输入。
    pub command: ExecutionCommandInput,
    /// 当前实例。
    pub instance: ApprovalProcessInstance,
    /// 当前执行。
    pub current: ApprovalNodeExecution,
    /// 任务 ID。
    pub work_item_id: String,
    /// 任务责任人。
    pub task_owner_id: String,
    /// 实例节点当前审批人。
    pub instance_assignee_id: String,
    /// 决定。
    pub decision: ApprovalDecision,
    /// 已 trim 原因。
    pub reason: Option<String>,
    /// 期望任务版本。
    pub expected_task_version: u64,
    /// 决定人。
    pub actor: ParticipantId,
    /// 下一执行主键。
    pub next_execution_id: ApprovalNodeExecutionId,
    /// 下一执行序号。
    pub next_execution_no: u32,
    /// 收据主键。
    pub receipt_id: ApprovalCommandReceiptId,
}

/// 规划决定。当前责任人失效时返回可提交的 blocked 计划。
///
/// # 参数
/// * `input` - 决定输入
///
/// # 错误
/// 责任不一致、异载荷冲突或引擎失败时返回错误。
pub fn prepare_decision(input: DecisionExecutionInput) -> Result<PreparedExecution> {
    let digest = decision_digest(
        &input.work_item_id,
        input.decision.as_str(),
        input.reason.as_deref(),
        input.expected_task_version,
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
    ensure_triple_responsibility(
        input.actor.as_str(),
        &input.task_owner_id,
        input.current.assignee_participant_id.as_str(),
        &input.instance_assignee_id,
    )?;
    let scope = input.current.base.id.clone();
    let plan = decide(
        input.instance,
        input.current,
        &input.command.graph,
        DecideCommand {
            decision: input.decision,
            reason: input.reason,
            actor: input.actor,
            current_eligibility: input.command.current_eligibility,
            next_eligibility: input.command.next_eligibility,
            next_execution_id: input.next_execution_id,
            next_execution_no: input.next_execution_no,
            now: input.command.now,
        },
    )
    .map_err(map_engine_error)?;
    let domain_action =
        (plan.commit == CommitRequired::TerminalApproved).then_some(DomainActionKind::FinalApprove);
    let receipt = receipt_from_plan(
        input.receipt_id,
        ApprovalCommandKind::SubmitDecision,
        scope,
        input.command.idempotency_key,
        digest,
        &plan,
        input.command.now,
    )?;
    Ok(PreparedExecution::Apply(Box::new(apply_plan(
        plan,
        receipt,
        domain_action,
    ))))
}

fn receipt_from_plan(
    id: ApprovalCommandReceiptId,
    kind: ApprovalCommandKind,
    scope: String,
    idempotency_key: String,
    digest: String,
    plan: &TransitionPlan,
    now: bpm::model::Timestamp,
) -> Result<ApprovalCommandReceipt> {
    ApprovalCommandReceipt::new(
        id,
        kind,
        scope,
        idempotency_key,
        digest,
        plan.instance.base.id.clone(),
        now,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 当前责任人失效的决定必须作为可提交 blocked 结果，不得回滚。
///
/// # 参数
/// * `prepared` - 编排结果
///
/// # 返回
/// blocked 提交返回 `true`。
pub fn decision_commits_blocked(prepared: &PreparedExecution) -> bool {
    match prepared {
        PreparedExecution::Apply(writes) => writes.commit == bpm::engine::CommitRequired::Blocked,
        PreparedExecution::Replay { .. } => false,
    }
}
