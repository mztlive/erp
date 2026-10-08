//! 启动审批的单事务编排核心。

use bpm::engine::{StartAssigneeBinding, StartCommand, TransitionPlan, start};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{ApprovalCommandReceipt, ParticipantId, ProcessKind, SubjectRef, Timestamp};

use super::apply_plan::{DomainActionKind, apply_plan};
use super::idempotency::{
    PreparedCommandIdentity, ReceiptBranch, StartIdentityParams, start_identity, start_scope_candidates,
};
use super::{ExecutionCommandInput, PreparedExecution};
use crate::error::{Error, ErrorCode, Result};

/// 启动编排输入。
#[derive(Debug, Clone)]
pub struct StartExecutionInput {
    /// 公共命令输入。
    pub command: ExecutionCommandInput,
    /// 流程种类。
    pub process_kind: ProcessKind,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 绑定定义 ID。
    pub binding_id: String,
    /// 定义版本。
    pub definition_version: u32,
    /// 启动人。
    pub actor: ParticipantId,
    /// 实例主键。
    pub instance_id: ApprovalProcessInstanceId,
    /// 入口执行主键。
    pub entry_execution_id: ApprovalNodeExecutionId,
    /// 收据主键。
    pub receipt_id: ApprovalCommandReceiptId,
    /// 节点绑定。
    pub bindings: Vec<StartAssigneeBinding>,
}

/// 规划启动：先处理收据，再调用 BPM 并展开写入。
///
/// # 参数
/// * `input` - 启动输入
///
/// # 返回
/// 同载荷返回 `PreparedExecution::Replay`；否则返回待应用的启动计划。
///
/// # 错误
/// 命令身份或 scope 不合法、异载荷冲突、引擎失败或收据构造失败时返回错误。
pub fn prepare_start(input: StartExecutionInput) -> Result<PreparedExecution> {
    let identity = start_identity(StartIdentityParams {
        idempotency_key: input.command.idempotency_key.clone(),
        process_kind: input.process_kind.as_str(),
        subject_kind: input.subject.subject_kind(),
        subject_id: input.subject.subject_id(),
        subject_version: input.subject_version,
        binding_id: &input.binding_id,
        definition_version: input.definition_version,
        actor_participant_id: input.actor.as_str(),
    })?;
    prepare_start_with_identity(input, identity)
}

/// 使用业务域已经签署的启动命令身份规划启动。
///
/// 专属启动载荷可以扩展 digest 字段，但必须复用统一 Start kind、规范 key 与
/// V3 scope；本入口在调用 BPM 前固定验证这些不可变身份。
///
/// # 参数
/// * `input` - 启动输入。
/// * `identity` - 调用方已签署的启动命令身份。
///
/// # 返回
/// 同载荷返回 `PreparedExecution::Replay`；否则返回待应用的启动计划。
///
/// # 错误
/// 身份种类、幂等键或 scope 与输入不一致、缺少 V3 scope、异载荷冲突、引擎失败或收据构造失败时返回错误。
pub fn prepare_start_with_identity(
    input: StartExecutionInput,
    identity: PreparedCommandIdentity,
) -> Result<PreparedExecution> {
    if identity.current().command_kind() != bpm::model::types::ApprovalCommandKind::StartApproval
        || identity.idempotency_key() != &input.command.idempotency_key
    {
        return Err(Error::ValidationError("启动命令身份与输入不一致".to_string()));
    }
    let expected_scope = start_scope_candidates(
        input.process_kind.as_str(),
        input.subject.subject_kind(),
        input.subject.subject_id(),
        input.subject_version,
    )?
    .into_iter()
    .next()
    .ok_or_else(|| Error::Internal("启动命令缺少 V3 scope".to_string()))?;
    if identity.current().scope().as_str() != expected_scope {
        return Err(Error::ValidationError("启动命令 scope 与业务主体不一致".to_string()));
    }
    match identity.classify(input.command.receipt.as_ref()) {
        ReceiptBranch::PayloadConflict => return Err(super::idempotency::payload_conflict_error()),
        ReceiptBranch::SamePayload(receipt) => {
            return Ok(PreparedExecution::Replay { receipt: receipt.clone() });
        },
        ReceiptBranch::Fresh => {},
    }
    let plan = start(
        StartCommand {
            instance_id: input.instance_id,
            process_kind: input.process_kind,
            subject: input.subject,
            subject_version: input.subject_version,
            started_by: input.actor,
            entry_execution_id: input.entry_execution_id,
            now: input.command.now,
        },
        &input.command.graph,
        &input.bindings,
    )
    .map_err(map_engine_error)?;
    let receipt = build_receipt(input.receipt_id, identity.current(), &plan, input.command.now)?;
    Ok(PreparedExecution::Apply(Box::new(apply_plan(plan, receipt, Some(DomainActionKind::Start)))))
}

/// 由计划构造收据。
fn build_receipt(
    id: ApprovalCommandReceiptId,
    identity: &bpm::model::ApprovalCommandIdentity,
    plan: &TransitionPlan,
    now: Timestamp,
) -> Result<ApprovalCommandReceipt> {
    ApprovalCommandReceipt::new(id, identity, plan.instance.base.id.clone(), now)
        .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 将引擎错误映射为服务错误。不可提交错误为内部错误。
///
/// # 参数
/// * `error` - BPM 引擎错误。
///
/// # 返回
/// `Uncommittable` 映射为 `Internal`，`InvalidCommand` 映射为 `ValidationError`，`GraphCorrupted` 映射为受阻审批码，`Model` 映射为 `BusinessLogicError`。
///
/// # 错误
/// 不返回错误。
pub fn map_engine_error(error: bpm::engine::EngineError) -> Error {
    match error {
        bpm::engine::EngineError::Uncommittable(message) => Error::Internal(message.to_string()),
        bpm::engine::EngineError::InvalidCommand(message) => Error::ValidationError(message.to_string()),
        bpm::engine::EngineError::GraphCorrupted => {
            Error::from_approval_code(ErrorCode::ApprovalInstanceBlocked)
        },
        bpm::engine::EngineError::Model(error) => Error::BusinessLogicError(error.to_string()),
    }
}
