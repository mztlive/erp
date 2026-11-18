//! 启动审批的单事务编排核心。

use bpm::engine::{StartAssigneeBinding, StartCommand, TransitionPlan, start};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{ApprovalCommandReceipt, ModelError, ParticipantId, ProcessKind, SubjectRef, Timestamp};

use super::apply_plan::{DomainActionKind, apply_plan};
use super::idempotency::{
    PreparedCommandIdentity, ReceiptBranch, StartIdentityParams, start_identity, start_scope_candidates,
};
use super::{ExecutionCommandInput, PreparedExecution};
use crate::error::{Error, ErrorCode, Result};

/// 启动编排输入。
#[derive(Debug, Clone)]
pub struct StartExecutionInput {
    /// 本次重新提交说明，参与启动命令摘要。
    pub resubmission_reason: Option<String>,
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
        resubmission_reason: input.resubmission_reason.as_deref(),
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
        return Err(Error::ValidationError("本次审批与所选单据不一致，请刷新后重新选择单据".to_string()));
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
        .map_err(approval_receipt_error)
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
        bpm::engine::EngineError::InvalidCommand(detail) => {
            tracing::debug!(detail, "审批引擎拒绝操作");
            let message = match detail {
                "取消原因不能为空" => "请填写取消原因",
                "驳回原因不能为空" => "请填写驳回原因",
                "原审批人资格尚未恢复" => "原审批人仍不具备审批资格，请先恢复其资格",
                "当前或签审批人资格已失效" => "当前审批人已不具备审批资格，请联系审批管理员",
                "启动时全部审批人必须有效，不得创建受阻实例" => {
                    "部分审批人不可用，请检查流程中的审批人设置后再提交"
                },
                "节点审批人数必须为 1 至 50" => "每个审批节点须配置 1 至 50 名审批人",
                "或签资格不能重复或嵌套" => "或签节点不可重复选择审批人，也不可再包含或签组",
                "审批定义没有节点，无法启动" | "审批定义缺少入口节点" => {
                    "审批流程没有可用的起始节点，请联系审批管理员检查流程配置"
                },
                "节点审批人绑定缺失或重复" | "入口节点缺少审批人绑定" => {
                    "审批节点未正确设置审批人，请检查流程配置后再提交"
                },
                "终态实例不得取消" => "该审批已结束，不能取消，请刷新后查看结果",
                "终态实例不得提交决定" => "该审批已结束，不能再审批，请刷新后查看结果",
                "执行不是实例当前令牌" | "只有活动执行可以接受决定" => {
                    "该审批已不在此处理环节，请刷新后核对当前待办"
                },
                _ => "当前审批资料或流程设置不符合此操作要求，请刷新核对；如仍失败，请联系审批管理员",
            };
            Error::ValidationError(message.into())
        },
        bpm::engine::EngineError::GraphCorrupted => {
            Error::from_approval_code(ErrorCode::ApprovalInstanceBlocked)
        },
        bpm::engine::EngineError::Model(error) => Error::BusinessLogicError(bpm_model_message(&error).into()),
    }
}

pub(super) fn approval_receipt_error(error: ModelError) -> Error {
    tracing::debug!(%error, "无法生成审批操作结果记录");
    Error::ValidationError("无法记录本次审批操作，请刷新核对审批状态；如仍失败，请联系审批管理员".into())
}

pub(crate) fn bpm_model_message(error: &ModelError) -> &'static str {
    tracing::debug!(%error, "审批模型拒绝操作");
    match error {
        ModelError::InvalidField(_) => "审批资料不完整或格式不正确，请核对后再操作",
        ModelError::InvalidStatus("已最终通过的审批实例不得撤回") => "审批已通过，不能再撤回",
        ModelError::InvalidStatus("已最终驳回的审批实例不得撤回") => "审批已驳回，不能再撤回",
        ModelError::InvalidStatus("已取消的审批实例不得重复撤回") => "审批已取消，不能重复撤回",
        ModelError::InvalidStatus(_) => "当前审批状态不允许此操作，请刷新后核对",
        ModelError::InvalidTransition(_) => "当前审批流程无法进入下一环节，请联系审批管理员检查流程设置",
        ModelError::ExecutionAlreadyEnded => "该审批环节已经结束，请刷新后查看审批结果",
        ModelError::Overflow(_) => "审批轮次或版本已达到上限，请联系审批管理员",
        ModelError::CommandReceiptConflict => "本次提交编号与原审批操作不一致，请先核对审批状态",
        ModelError::InvalidTimestamp => "本次审批操作时间无效，请联系管理员",
    }
}

#[cfg(test)]
mod tests {
    use bpm::engine::EngineError;

    use super::{Error, ModelError, map_engine_error};

    #[test]
    fn engine_rejections_preserve_classification_and_hide_model_terms() {
        let command_error = map_engine_error(EngineError::InvalidCommand("执行不是实例当前令牌"));
        let model_error = map_engine_error(EngineError::Model(ModelError::Overflow("execution_no")));

        assert!(matches!(command_error, Error::ValidationError(message) if message.contains("当前待办")));
        assert!(
            matches!(model_error, Error::BusinessLogicError(message) if message.contains("达到上限") && !message.contains("execution_no"))
        );
    }
}
