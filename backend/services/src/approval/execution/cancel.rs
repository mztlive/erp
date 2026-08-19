//! 业务撤回与受阻取消编排。

use bpm::engine::{cancel, CancelCommand};
use bpm::ids::ApprovalCommandReceiptId;
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId};

use super::apply_plan::{apply_plan, DomainActionKind};
use super::authorization::requires_blocked_cancel;
use super::idempotency::{cancel_blocked_digest, cancel_digest, classify_receipt, ReceiptBranch};
use super::start::map_engine_error;
use super::{ExecutionCommandInput, PreparedExecution};
use crate::errors::{Error, Result};

/// 取消编排输入。
#[derive(Debug, Clone)]
pub struct CancelExecutionInput {
    /// 公共输入。
    pub command: ExecutionCommandInput,
    /// 当前实例。
    pub instance: ApprovalProcessInstance,
    /// 当前执行。
    pub current: ApprovalNodeExecution,
    /// 提交版本。
    pub subject_version: u32,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 可空任务版本。
    pub expected_task_version: Option<u64>,
    /// 非空原因。
    pub reason: String,
    /// 取消人。
    pub actor: ParticipantId,
    /// 是否关闭开放任务。
    pub close_open_task: bool,
    /// 是否受阻取消端口。
    pub blocked_port: bool,
    /// 收据主键。
    pub receipt_id: ApprovalCommandReceiptId,
}

/// 规划取消。人员失效走业务取消，非人员一致性 blocker 必须走受阻取消。
///
/// # 参数
/// * `input` - 取消输入
///
/// # 错误
/// 端口与 blocker 类别不匹配、异载荷冲突或引擎失败时返回错误。
pub fn prepare_cancel(input: CancelExecutionInput) -> Result<PreparedExecution> {
    let digest = if input.blocked_port {
        let blocker = input
            .instance
            .blocker_code
            .or(input.current.blocker_code)
            .ok_or_else(|| Error::ValidationError("受阻取消缺少 blocker".to_string()))?;
        if !requires_blocked_cancel(blocker) {
            return Err(Error::ValidationError("人员失效不得走受阻取消".to_string()));
        }
        cancel_blocked_digest(
            blocker.as_str(),
            input.expected_instance_version,
            input.expected_execution_version,
            input.expected_task_version,
            &input.reason,
            input.actor.as_str(),
        )
    } else {
        if input.instance.blocker_code.is_some_and(requires_blocked_cancel) {
            return Err(Error::ValidationError(
                "非人员一致性阻塞只能走受阻取消".to_string(),
            ));
        }
        cancel_digest(
            input.subject_version,
            input.expected_instance_version,
            input.expected_execution_version,
            input.expected_task_version,
            &input.reason,
            input.actor.as_str(),
        )
    };
    match classify_receipt(input.command.receipt.as_ref(), &digest) {
        ReceiptBranch::PayloadConflict => return Err(super::idempotency::payload_conflict_error()),
        ReceiptBranch::SamePayload(receipt) => {
            return Ok(PreparedExecution::Replay {
                receipt: receipt.clone(),
            });
        }
        ReceiptBranch::Fresh => {}
    }
    let kind = if input.blocked_port {
        ApprovalCommandKind::CancelBlocked
    } else {
        ApprovalCommandKind::CancelApproval
    };
    let plan = cancel(
        input.instance,
        input.current,
        CancelCommand {
            actor: input.actor,
            reason: input.reason,
            close_open_task: input.close_open_task,
            now: input.command.now,
        },
    )
    .map_err(map_engine_error)?;
    let receipt = ApprovalCommandReceipt::new(
        input.receipt_id,
        kind,
        plan.instance.base.id.clone(),
        input.command.idempotency_key,
        digest,
        plan.instance.base.id.clone(),
        input.command.now,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(PreparedExecution::Apply(Box::new(apply_plan(
        plan,
        receipt,
        Some(DomainActionKind::Cancel),
    ))))
}
