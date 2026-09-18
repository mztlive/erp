//! 决定、取消、恢复与绑定升级命令身份。

use bpm::model::types::ApprovalCommandKind;
use bpm::model::{CanonicalCommandPayload, CommandPayloadField, IdempotencyKey};

use super::identity::{LegacyReceiptIdentity, PreparedCommandIdentity, current_identity};
use super::legacy::{
    legacy_cancel_blocked_digest, legacy_cancel_blocked_digest_v2, legacy_cancel_digest,
    legacy_decision_digest, legacy_decision_digest_v2, legacy_document_cancel_digest, legacy_resume_digest,
};
use crate::error::Result;

const DECISION_DOMAIN: &str = "APPROVAL_EXECUTION_DECISION";
const CANCEL_DOMAIN: &str = "APPROVAL_EXECUTION_CANCEL";
const DOCUMENT_CANCEL_DOMAIN: &str = "APPROVAL_EXECUTION_DOCUMENT_CANCEL";
const RESUME_DOMAIN: &str = "APPROVAL_EXECUTION_RESUME_ORIGINAL_APPROVER";
const CANCEL_BLOCKED_DOMAIN: &str = "APPROVAL_EXECUTION_CANCEL_BLOCKED";
const UPGRADE_BINDING_DOMAIN: &str = "APPROVAL_EXECUTION_UPGRADE_BINDING";

/// 形成审批决定的 V3 身份，并精确登记 V2 与无前缀历史摘要。
pub fn decision_identity(
    idempotency_key: IdempotencyKey,
    execution_id: &str,
    work_item_id: &str,
    decision: &str,
    reason: Option<&str>,
    expected_task_version: u64,
    actor_id: &str,
) -> Result<PreparedCommandIdentity> {
    let scope_payload = CanonicalCommandPayload::new().field(CommandPayloadField::Text(execution_id));
    let digest_payload = CanonicalCommandPayload::new()
        .field(CommandPayloadField::Text(work_item_id))
        .field(CommandPayloadField::Text(decision))
        .field(CommandPayloadField::OptionalText(reason))
        .field(CommandPayloadField::U64(expected_task_version))
        .field(CommandPayloadField::Text(actor_id));
    let current = current_identity(
        ApprovalCommandKind::SubmitDecision,
        DECISION_DOMAIN,
        idempotency_key,
        scope_payload,
        digest_payload,
    )?;
    let legacy_scope = execution_id.to_string();
    Ok(PreparedCommandIdentity::new(
        current,
        vec![
            LegacyReceiptIdentity::exact(
                legacy_scope.clone(),
                legacy_decision_digest_v2(work_item_id, decision, reason, expected_task_version, actor_id),
            ),
            LegacyReceiptIdentity::exact(
                legacy_scope,
                legacy_decision_digest(work_item_id, decision, reason, expected_task_version, actor_id),
            ),
        ],
    ))
}

/// 通用审批取消命令身份参数。
///
/// # 用途
/// 打包 [`cancel_identity`] 所需的 scope/digest 字段。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 不含单据版本；单据撤回应走 [`document_cancel_identity`]。
#[derive(Debug, Clone)]
pub struct CancelIdentityParams<'a> {
    /// 规范化幂等键。
    pub idempotency_key: IdempotencyKey,
    /// 审批流程实例 ID。
    pub instance_id: &'a str,
    /// 冻结主体版本。
    pub subject_version: u32,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 期望任务版本；无开放任务时为 `None`。
    pub expected_task_version: Option<u64>,
    /// 已规范化取消原因。
    pub reason: &'a str,
    /// 操作人 ID。
    pub actor_id: &'a str,
}

/// 形成通用审批取消的 V3 身份与无前缀历史身份。
///
/// # 用途
/// 构造通用 Cancel 命令的 V3 身份并登记无前缀历史候选。
///
/// # 参数
/// * `params` - 取消命令 scope/digest 字段
///
/// # 返回
/// 当前 V3 身份与精确 legacy 候选。
///
/// # 错误
/// 幂等键或载荷字段非法时返回校验错误。
///
/// # 关键业务约束
/// 受阻取消与单据撤回不得复用本身份。
pub fn cancel_identity(params: CancelIdentityParams<'_>) -> Result<PreparedCommandIdentity> {
    let CancelIdentityParams {
        idempotency_key,
        instance_id,
        subject_version,
        expected_instance_version,
        expected_execution_version,
        expected_task_version,
        reason,
        actor_id,
    } = params;
    let current = current_identity(
        ApprovalCommandKind::CancelApproval,
        CANCEL_DOMAIN,
        idempotency_key,
        instance_scope_payload(instance_id),
        cancel_digest_payload(
            subject_version,
            expected_instance_version,
            expected_execution_version,
            expected_task_version,
            reason,
            actor_id,
        ),
    )?;
    Ok(PreparedCommandIdentity::new(
        current,
        vec![LegacyReceiptIdentity::exact(
            instance_id,
            legacy_cancel_digest(
                subject_version,
                expected_instance_version,
                expected_execution_version,
                expected_task_version,
                reason,
                actor_id,
            ),
        )],
    ))
}

/// 业务单据普通撤回命令身份参数。
///
/// # 用途
/// 打包 [`document_cancel_identity`] 所需的 scope/digest 字段。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// digest 必须同时绑定单据版本与运行实例版本。
#[derive(Debug, Clone)]
pub struct DocumentCancelIdentityParams<'a> {
    /// 规范化幂等键。
    pub idempotency_key: IdempotencyKey,
    /// 审批流程实例 ID。
    pub instance_id: &'a str,
    /// 冻结主体版本。
    pub subject_version: u32,
    /// 期望业务单据版本。
    pub expected_document_version: u64,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 期望任务版本；无开放任务时为 `None`。
    pub expected_task_version: Option<u64>,
    /// 已规范化取消原因。
    pub reason: &'a str,
    /// 操作人 ID。
    pub actor_id: &'a str,
}

/// 形成业务单据普通撤回的 V3 身份与已知无前缀历史身份。
///
/// # 用途
/// 构造单据撤回命令的 V3 身份并登记无前缀历史候选。
///
/// # 参数
/// * `params` - 单据撤回 scope/digest 字段
///
/// # 返回
/// 当前 V3 身份与精确 legacy 候选。
///
/// # 错误
/// 幂等键或载荷字段非法时返回校验错误。
///
/// # 关键业务约束
/// 不得与通用取消或受阻取消共享 digest 域。
pub fn document_cancel_identity(params: DocumentCancelIdentityParams<'_>) -> Result<PreparedCommandIdentity> {
    let DocumentCancelIdentityParams {
        idempotency_key,
        instance_id,
        subject_version,
        expected_document_version,
        expected_instance_version,
        expected_execution_version,
        expected_task_version,
        reason,
        actor_id,
    } = params;
    let digest_payload = CanonicalCommandPayload::new()
        .field(CommandPayloadField::U32(subject_version))
        .field(CommandPayloadField::U64(expected_document_version))
        .field(CommandPayloadField::U64(expected_instance_version))
        .field(CommandPayloadField::U64(expected_execution_version))
        .field(CommandPayloadField::OptionalU64(expected_task_version))
        .field(CommandPayloadField::Text(reason.trim()))
        .field(CommandPayloadField::Text(actor_id.trim()));
    let current = current_identity(
        ApprovalCommandKind::CancelApproval,
        DOCUMENT_CANCEL_DOMAIN,
        idempotency_key,
        instance_scope_payload(instance_id),
        digest_payload,
    )?;
    Ok(PreparedCommandIdentity::new(
        current,
        vec![LegacyReceiptIdentity::exact(
            instance_id,
            legacy_document_cancel_digest(
                subject_version,
                expected_document_version,
                expected_instance_version,
                expected_execution_version,
                expected_task_version,
                reason,
                actor_id,
            ),
        )],
    ))
}

/// 形成原审批人恢复的 V3 身份与无前缀历史身份。
pub fn resume_identity(
    idempotency_key: IdempotencyKey,
    instance_id: &str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_assignment_version: u64,
    expected_closed_task_version: Option<u64>,
    actor_id: &str,
) -> Result<PreparedCommandIdentity> {
    let digest_payload = CanonicalCommandPayload::new()
        .field(CommandPayloadField::U64(expected_instance_version))
        .field(CommandPayloadField::U64(expected_execution_version))
        .field(CommandPayloadField::U64(expected_assignment_version))
        .field(CommandPayloadField::OptionalU64(expected_closed_task_version))
        .field(CommandPayloadField::Text(actor_id));
    let current = current_identity(
        ApprovalCommandKind::ResumeApprover,
        RESUME_DOMAIN,
        idempotency_key,
        instance_scope_payload(instance_id),
        digest_payload,
    )?;
    Ok(PreparedCommandIdentity::new(
        current,
        vec![LegacyReceiptIdentity::exact(
            instance_id,
            legacy_resume_digest(
                expected_instance_version,
                expected_execution_version,
                expected_assignment_version,
                expected_closed_task_version,
                actor_id,
            ),
        )],
    ))
}

/// 受阻取消命令身份参数。
///
/// # 用途
/// 打包 [`cancel_blocked_identity`] 所需的 scope/digest 字段。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// digest 必须绑定 blocker 代码，不得与普通取消共享。
#[derive(Debug, Clone)]
pub struct CancelBlockedIdentityParams<'a> {
    /// 规范化幂等键。
    pub idempotency_key: IdempotencyKey,
    /// 审批流程实例 ID。
    pub instance_id: &'a str,
    /// 阻塞代码稳定串。
    pub blocker: &'a str,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 期望任务版本；无开放任务时为 `None`。
    pub expected_task_version: Option<u64>,
    /// 已规范化取消原因。
    pub reason: &'a str,
    /// 操作人 ID。
    pub actor_id: &'a str,
}

/// 形成受阻取消的 V3 身份，并精确登记 V2 与无前缀历史摘要。
///
/// # 用途
/// 构造 CancelBlocked 命令的 V3 身份并登记 V2/legacy 候选。
///
/// # 参数
/// * `params` - 受阻取消 scope/digest 字段
///
/// # 返回
/// 当前 V3 身份与精确 legacy 候选。
///
/// # 错误
/// 幂等键或载荷字段非法时返回校验错误。
///
/// # 关键业务约束
/// 原审批人可恢复时不得走受阻取消身份。
pub fn cancel_blocked_identity(params: CancelBlockedIdentityParams<'_>) -> Result<PreparedCommandIdentity> {
    let CancelBlockedIdentityParams {
        idempotency_key,
        instance_id,
        blocker,
        expected_instance_version,
        expected_execution_version,
        expected_task_version,
        reason,
        actor_id,
    } = params;
    let digest_payload = CanonicalCommandPayload::new()
        .field(CommandPayloadField::Text(blocker))
        .field(CommandPayloadField::U64(expected_instance_version))
        .field(CommandPayloadField::U64(expected_execution_version))
        .field(CommandPayloadField::OptionalU64(expected_task_version))
        .field(CommandPayloadField::Text(reason))
        .field(CommandPayloadField::Text(actor_id));
    let current = current_identity(
        ApprovalCommandKind::CancelBlocked,
        CANCEL_BLOCKED_DOMAIN,
        idempotency_key,
        instance_scope_payload(instance_id),
        digest_payload,
    )?;
    Ok(PreparedCommandIdentity::new(
        current,
        vec![
            LegacyReceiptIdentity::exact(
                instance_id,
                legacy_cancel_blocked_digest_v2(
                    blocker,
                    expected_instance_version,
                    expected_execution_version,
                    expected_task_version,
                    reason,
                    actor_id,
                ),
            ),
            LegacyReceiptIdentity::exact(
                instance_id,
                legacy_cancel_blocked_digest(
                    blocker,
                    expected_instance_version,
                    expected_execution_version,
                    expected_task_version,
                    reason,
                    actor_id,
                ),
            ),
        ],
    ))
}

/// 形成未提交业务单据绑定升级的唯一 V3 命令身份。
///
/// 升级命令在本开发期没有已发布历史 writer，不登记任何 legacy 候选。scope
/// 精确绑定单据类型与 ID；digest 同时绑定 scope 字段、业务对象版本、绑定版本、
/// 规范化原因和实际操作人。
pub fn upgrade_binding_identity(
    document_type: &str,
    document_id: &str,
    expected_business_object_version: u64,
    expected_binding_version: u64,
    normalized_reason: &str,
    actor_id: &str,
    idempotency_key: IdempotencyKey,
) -> Result<PreparedCommandIdentity> {
    let scope_payload = CanonicalCommandPayload::new()
        .field(CommandPayloadField::Text(document_type))
        .field(CommandPayloadField::Text(document_id));
    let digest_payload = CanonicalCommandPayload::new()
        .field(CommandPayloadField::Text(document_type))
        .field(CommandPayloadField::Text(document_id))
        .field(CommandPayloadField::U64(expected_business_object_version))
        .field(CommandPayloadField::U64(expected_binding_version))
        .field(CommandPayloadField::Text(normalized_reason))
        .field(CommandPayloadField::Text(actor_id));
    let current = current_identity(
        ApprovalCommandKind::UpgradeBinding,
        UPGRADE_BINDING_DOMAIN,
        idempotency_key,
        scope_payload,
        digest_payload,
    )?;
    Ok(PreparedCommandIdentity::new(current, Vec::new()))
}

fn instance_scope_payload(instance_id: &str) -> CanonicalCommandPayload {
    CanonicalCommandPayload::new().field(CommandPayloadField::Text(instance_id))
}

fn cancel_digest_payload(
    subject_version: u32,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> CanonicalCommandPayload {
    CanonicalCommandPayload::new()
        .field(CommandPayloadField::U32(subject_version))
        .field(CommandPayloadField::U64(expected_instance_version))
        .field(CommandPayloadField::U64(expected_execution_version))
        .field(CommandPayloadField::OptionalU64(expected_task_version))
        .field(CommandPayloadField::Text(reason))
        .field(CommandPayloadField::Text(actor_id))
}
