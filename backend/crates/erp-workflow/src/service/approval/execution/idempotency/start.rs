//! 启动命令的当前 V3 身份与历史无前缀候选。

use bpm::model::types::ApprovalCommandKind;
use bpm::model::{CanonicalCommandPayload, CommandPayloadField, CommandScope, IdempotencyKey};

use super::identity::{LegacyReceiptIdentity, PreparedCommandIdentity, current_identity};
use super::legacy::{legacy_start_digest, legacy_start_scope};
use crate::error::{Error, Result};

const START_DOMAIN: &str = "APPROVAL_EXECUTION_START";

/// 返回启动命令的当前 V3 scope 与历史无前缀 scope。
///
/// 此函数供必须先查 receipt、尚未加载定义绑定与 digest 字段的业务端口使用。
pub fn start_scope_candidates(
    process_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    subject_version: u32,
) -> Result<Vec<String>> {
    let current = CommandScope::v3(
        ApprovalCommandKind::StartApproval,
        START_DOMAIN,
        &start_scope_payload(process_kind, subject_kind, subject_id, subject_version),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    let legacy = legacy_start_scope(process_kind, subject_kind, subject_id, subject_version);
    Ok(vec![current.as_str().to_string(), legacy])
}

/// 通用启动命令身份参数。
///
/// # 用途
/// 打包 [`start_identity`] 所需的 scope/digest 字段。
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
/// 绑定 ID、定义版本与操作人进入 STANDARD 变体 digest。
#[derive(Debug, Clone)]
pub struct StartIdentityParams<'a> {
    /// 规范化幂等键。
    pub idempotency_key: IdempotencyKey,
    /// 流程种类稳定码。
    pub process_kind: &'a str,
    /// 主体种类。
    pub subject_kind: &'a str,
    /// 主体主键。
    pub subject_id: &'a str,
    /// 冻结主体版本。
    pub subject_version: u32,
    /// 审批定义绑定 ID。
    pub binding_id: &'a str,
    /// 审批定义版本。
    pub definition_version: u32,
    /// 启动操作人参与者 ID。
    pub actor_participant_id: &'a str,
}

/// 形成启动命令的当前 V3 身份与历史无前缀身份。
///
/// # 用途
/// 构造通用 Start 命令的 V3 身份并登记历史 STANDARD writer 候选。
///
/// # 参数
/// * `params` - 启动命令 scope/digest 字段
///
/// # 返回
/// 当前 V3 身份与精确 legacy 候选。
///
/// # 错误
/// 幂等键或载荷字段非法时返回校验错误。
///
/// # 关键业务约束
/// 专属启动命令应走 [`specialized_start_identity`]，不得复用 STANDARD digest。
pub fn start_identity(params: StartIdentityParams<'_>) -> Result<PreparedCommandIdentity> {
    let StartIdentityParams {
        idempotency_key,
        process_kind,
        subject_kind,
        subject_id,
        subject_version,
        binding_id,
        definition_version,
        actor_participant_id,
    } = params;
    let current = specialized_start_identity(
        idempotency_key,
        process_kind,
        subject_kind,
        subject_id,
        subject_version,
        "STANDARD",
        vec![
            CommandPayloadField::Text(binding_id),
            CommandPayloadField::U32(definition_version),
            CommandPayloadField::U32(subject_version),
            CommandPayloadField::Text(actor_participant_id),
        ],
    )?;
    Ok(current.with_legacy(legacy_standard_start_receipt_identity(
        process_kind,
        subject_kind,
        subject_id,
        subject_version,
        binding_id,
        definition_version,
        actor_participant_id,
    )))
}

/// 形成带显式变体与 typed 字段序列的启动命令 V3 身份。
///
/// 业务域专属启动命令必须固定 `variant` 和字段顺序；本函数统一复用 Start
/// scope 协议，避免专属 digest 破坏 receipt-first 查询。历史 writer 必须由
/// 调用方显式追加精确成对候选。
pub fn specialized_start_identity<'a>(
    idempotency_key: IdempotencyKey,
    process_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    subject_version: u32,
    variant: &'a str,
    digest_fields: Vec<CommandPayloadField<'a>>,
) -> Result<PreparedCommandIdentity> {
    if variant.is_empty() || variant.trim() != variant {
        return Err(Error::ValidationError("启动命令摘要变体无效".to_string()));
    }
    let digest_payload = CanonicalCommandPayload::new()
        .field(CommandPayloadField::Text(variant))
        .field(CommandPayloadField::Sequence(digest_fields));
    let current = current_identity(
        ApprovalCommandKind::StartApproval,
        START_DOMAIN,
        idempotency_key,
        start_scope_payload(process_kind, subject_kind, subject_id, subject_version),
        digest_payload,
    )?;
    Ok(PreparedCommandIdentity::new(current, Vec::new()))
}

/// 为已知历史启动 writer 形成 scope/digest 精确候选。
pub fn legacy_start_receipt_identity(
    process_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    subject_version: u32,
    legacy_digest: impl Into<String>,
) -> LegacyReceiptIdentity {
    LegacyReceiptIdentity::exact(
        legacy_start_scope(process_kind, subject_kind, subject_id, subject_version),
        legacy_digest,
    )
}

/// 为历史通用 Start writer 形成旧 scope 与旧 digest 的精确成对候选。
pub fn legacy_standard_start_receipt_identity(
    process_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    subject_version: u32,
    binding_id: &str,
    definition_version: u32,
    actor_participant_id: &str,
) -> LegacyReceiptIdentity {
    legacy_start_receipt_identity(
        process_kind,
        subject_kind,
        subject_id,
        subject_version,
        legacy_start_digest(binding_id, definition_version, subject_version, actor_participant_id),
    )
}

fn start_scope_payload(
    process_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    subject_version: u32,
) -> CanonicalCommandPayload {
    CanonicalCommandPayload::new()
        .field(CommandPayloadField::Text(process_kind))
        .field(CommandPayloadField::Text(subject_kind))
        .field(CommandPayloadField::Text(subject_id))
        .field(CommandPayloadField::U32(subject_version))
}
