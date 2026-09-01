//! 审批运行命令的统一幂等身份、当前 V3 协议与历史精确候选。

use bpm::model::types::ApprovalCommandKind;
use bpm::model::{
    ApprovalCommandIdentity, ApprovalCommandReceipt, CanonicalCommandPayload, CommandPayloadField,
    CommandScope, IdempotencyKey,
};
use sha2::{Digest, Sha256};
use std::time::Duration;

use crate::errors::{Error, ErrorCode, Result};

const START_DOMAIN: &str = "APPROVAL_EXECUTION_START";
const DECISION_DOMAIN: &str = "APPROVAL_EXECUTION_DECISION";
const CANCEL_DOMAIN: &str = "APPROVAL_EXECUTION_CANCEL";
const DOCUMENT_CANCEL_DOMAIN: &str = "APPROVAL_EXECUTION_DOCUMENT_CANCEL";
const RESUME_DOMAIN: &str = "APPROVAL_EXECUTION_RESUME_ORIGINAL_APPROVER";
const CANCEL_BLOCKED_DOMAIN: &str = "APPROVAL_EXECUTION_CANCEL_BLOCKED";
const UPGRADE_BINDING_DOMAIN: &str = "APPROVAL_EXECUTION_UPGRADE_BINDING";
const V2_DIGEST_PREFIX: &str = "v2:";

/// 收据查找后的分支。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptBranch<'a> {
    /// 收据不存在，允许执行命令。
    Fresh,
    /// 同载荷，只允许授权回读。
    SamePayload(&'a ApprovalCommandReceipt),
    /// 异载荷或身份降级冲突。
    PayloadConflict,
}

/// 一个历史收据只允许以原 scope 与原 digest 成对匹配。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyReceiptIdentity {
    scope: String,
    digest: String,
}

impl LegacyReceiptIdentity {
    /// 创建一个已知历史 writer 的精确身份候选。
    pub fn exact(scope: impl Into<String>, digest: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            digest: digest.into(),
        }
    }
}

/// 一个审批运行命令的当前 V3 身份及其有限历史读取候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCommandIdentity {
    current: ApprovalCommandIdentity,
    legacy: Vec<LegacyReceiptIdentity>,
}

impl PreparedCommandIdentity {
    fn new(current: ApprovalCommandIdentity, legacy: Vec<LegacyReceiptIdentity>) -> Self {
        Self { current, legacy }
    }

    /// 返回当前 writer 唯一允许写入的 V3 身份。
    pub fn current(&self) -> &ApprovalCommandIdentity {
        &self.current
    }

    /// 返回已规范化幂等键。
    pub fn idempotency_key(&self) -> &IdempotencyKey {
        self.current.idempotency_key()
    }

    /// 返回 receipt 查询 scope，严格按 V3、已知历史格式顺序去重。
    pub fn scope_candidates(&self) -> Vec<&str> {
        let mut scopes = vec![self.current.scope().as_str()];
        for candidate in &self.legacy {
            if !scopes.contains(&candidate.scope.as_str()) {
                scopes.push(candidate.scope.as_str());
            }
        }
        scopes
    }

    /// 追加一个调用方拥有的已知历史 writer 身份。
    ///
    /// 仅用于兼容已经持久化的显式版本格式；不得把模糊组合、任意旧摘要或
    /// V3 scope/digest 的交叉组合登记为候选。
    pub fn with_legacy(mut self, candidate: LegacyReceiptIdentity) -> Self {
        if !self.legacy.contains(&candidate) {
            self.legacy.push(candidate);
        }
        self
    }

    /// 按完整命令身份分类收据。
    ///
    /// 当前 V3 scope 只接受当前 V3 digest。历史收据只接受登记时成对保存的
    /// scope 与 digest，禁止 scope/digest 交叉组合或 V3 降级匹配。
    pub fn classify<'a>(&self, receipt: Option<&'a ApprovalCommandReceipt>) -> ReceiptBranch<'a> {
        let Some(receipt) = receipt else {
            return ReceiptBranch::Fresh;
        };
        if receipt.command_kind != self.current.command_kind()
            || &receipt.idempotency_key != self.current.idempotency_key()
        {
            return ReceiptBranch::PayloadConflict;
        }
        if receipt.scope_id == self.current.scope().as_str() {
            return if receipt.payload_digest == self.current.digest().as_str() {
                ReceiptBranch::SamePayload(receipt)
            } else {
                ReceiptBranch::PayloadConflict
            };
        }
        if is_v3_hash(&receipt.scope_id) || is_v3_hash(&receipt.payload_digest) {
            return ReceiptBranch::PayloadConflict;
        }
        if self.legacy.iter().any(|candidate| {
            candidate.scope == receipt.scope_id && candidate.digest == receipt.payload_digest
        }) {
            ReceiptBranch::SamePayload(receipt)
        } else {
            ReceiptBranch::PayloadConflict
        }
    }
}

fn is_v3_hash(value: &str) -> bool {
    value
        .strip_prefix("v3:")
        .is_some_and(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// 在任何仓储查询前形成规范化幂等键。
///
/// # 错误
/// 空值、超长值或其他 BPM 值对象约束不满足时返回校验错误。
pub fn normalize_idempotency_key(raw: &str) -> Result<IdempotencyKey> {
    IdempotencyKey::parse(raw).map_err(|error| Error::ValidationError(error.to_string()))
}

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

/// 形成启动命令的当前 V3 身份与历史无前缀身份。
#[allow(clippy::too_many_arguments)]
pub fn start_identity(
    idempotency_key: IdempotencyKey,
    process_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    subject_version: u32,
    binding_id: &str,
    definition_version: u32,
    actor_participant_id: &str,
) -> Result<PreparedCommandIdentity> {
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
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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
        legacy_start_digest(
            binding_id,
            definition_version,
            subject_version,
            actor_participant_id,
        ),
    )
}

/// 形成审批决定的 V3 身份，并精确登记 V2 与无前缀历史摘要。
#[allow(clippy::too_many_arguments)]
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

/// 形成通用审批取消的 V3 身份与无前缀历史身份。
#[allow(clippy::too_many_arguments)]
pub fn cancel_identity(
    idempotency_key: IdempotencyKey,
    instance_id: &str,
    subject_version: u32,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> Result<PreparedCommandIdentity> {
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

/// 形成业务单据普通撤回的 V3 身份与已知无前缀历史身份。
#[allow(clippy::too_many_arguments)]
pub fn document_cancel_identity(
    idempotency_key: IdempotencyKey,
    instance_id: &str,
    subject_version: u32,
    expected_document_version: u64,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> Result<PreparedCommandIdentity> {
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
#[allow(clippy::too_many_arguments)]
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

/// 形成受阻取消的 V3 身份，并精确登记 V2 与无前缀历史摘要。
#[allow(clippy::too_many_arguments)]
pub fn cancel_blocked_identity(
    idempotency_key: IdempotencyKey,
    instance_id: &str,
    blocker: &str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> Result<PreparedCommandIdentity> {
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
#[allow(clippy::too_many_arguments)]
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

fn current_identity(
    kind: ApprovalCommandKind,
    domain: &str,
    key: IdempotencyKey,
    scope_payload: CanonicalCommandPayload,
    digest_payload: CanonicalCommandPayload,
) -> Result<ApprovalCommandIdentity> {
    ApprovalCommandIdentity::new(kind, domain, key, scope_payload, digest_payload)
        .map_err(|error| Error::ValidationError(error.to_string()))
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

/// 对已经无碰撞编码的历史文本计算稳定 SHA-256 摘要。
///
/// 仅供精确历史 writer 或调用方自有的显式版本格式使用；当前命令必须通过
/// [`ApprovalCommandIdentity`] 写入 V3 摘要。
pub(crate) fn legacy_payload_digest(canonical: &str) -> String {
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn legacy_canonical_payload(fields: &[&str]) -> String {
    fields
        .iter()
        .map(|field| {
            let trimmed = field.trim();
            if trimmed.is_empty() {
                "NULL"
            } else {
                trimmed
            }
        })
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

fn legacy_start_scope(
    process_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    subject_version: u32,
) -> String {
    legacy_canonical_payload(&[
        process_kind,
        subject_kind,
        subject_id,
        &subject_version.to_string(),
    ])
}

fn legacy_start_digest(
    binding_id: &str,
    definition_version: u32,
    subject_version: u32,
    actor_participant_id: &str,
) -> String {
    legacy_payload_digest(&legacy_canonical_payload(&[
        binding_id,
        &definition_version.to_string(),
        &subject_version.to_string(),
        actor_participant_id,
    ]))
}

fn legacy_decision_digest(
    work_item_id: &str,
    decision: &str,
    reason: Option<&str>,
    expected_task_version: u64,
    actor_id: &str,
) -> String {
    legacy_payload_digest(&legacy_canonical_payload(&[
        work_item_id,
        decision,
        reason.unwrap_or(""),
        &expected_task_version.to_string(),
        actor_id,
    ]))
}

fn legacy_cancel_digest(
    subject_version: u32,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> String {
    let task_version = expected_task_version
        .map(|value| value.to_string())
        .unwrap_or_default();
    legacy_payload_digest(&legacy_canonical_payload(&[
        &subject_version.to_string(),
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &task_version,
        reason,
        actor_id,
    ]))
}

#[allow(clippy::too_many_arguments)]
fn legacy_document_cancel_digest(
    subject_version: u32,
    expected_document_version: u64,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> String {
    let mut canonical = String::new();
    push_length_prefixed(&mut canonical, "DOCUMENT_CANCEL");
    push_length_prefixed(&mut canonical, "1");
    push_length_prefixed(&mut canonical, &subject_version.to_string());
    push_length_prefixed(&mut canonical, &expected_document_version.to_string());
    push_length_prefixed(&mut canonical, &expected_instance_version.to_string());
    push_length_prefixed(&mut canonical, &expected_execution_version.to_string());
    match expected_task_version {
        Some(value) => {
            push_length_prefixed(&mut canonical, "SOME");
            push_length_prefixed(&mut canonical, &value.to_string());
        }
        None => push_length_prefixed(&mut canonical, "NONE"),
    }
    push_length_prefixed(&mut canonical, reason.trim());
    push_length_prefixed(&mut canonical, actor_id.trim());
    legacy_payload_digest(&canonical)
}

fn push_length_prefixed(target: &mut String, value: &str) {
    target.push_str(&value.len().to_string());
    target.push(':');
    target.push_str(value);
}

fn legacy_resume_digest(
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_assignment_version: u64,
    expected_closed_task_version: Option<u64>,
    actor_id: &str,
) -> String {
    let task_version = expected_closed_task_version
        .map(|value| value.to_string())
        .unwrap_or_default();
    legacy_payload_digest(&legacy_canonical_payload(&[
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &expected_assignment_version.to_string(),
        &task_version,
        actor_id,
    ]))
}

fn legacy_cancel_blocked_digest(
    blocker: &str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> String {
    let task_version = expected_task_version
        .map(|value| value.to_string())
        .unwrap_or_default();
    legacy_payload_digest(&legacy_canonical_payload(&[
        blocker,
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &task_version,
        reason,
        actor_id,
    ]))
}

#[derive(Debug, Clone, Copy)]
enum LegacyV2Field<'a> {
    Text(&'a str),
    U64(u64),
    OptionalText(Option<&'a str>),
    OptionalU64(Option<u64>),
}

fn legacy_digest_v2(domain: &str, fields: &[LegacyV2Field<'_>]) -> String {
    fn update_text(hasher: &mut Sha256, value: &str) {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }

    let mut hasher = Sha256::new();
    hasher.update(b"erp.approval.command-digest");
    hasher.update([0, 2]);
    update_text(&mut hasher, domain);
    hasher.update((fields.len() as u64).to_be_bytes());
    for field in fields {
        match field {
            LegacyV2Field::Text(value) => {
                hasher.update([1]);
                update_text(&mut hasher, value);
            }
            LegacyV2Field::U64(value) => {
                hasher.update([2]);
                hasher.update(value.to_be_bytes());
            }
            LegacyV2Field::OptionalText(value) => {
                hasher.update([3]);
                match value {
                    Some(value) => {
                        hasher.update([1]);
                        update_text(&mut hasher, value);
                    }
                    None => hasher.update([0]),
                }
            }
            LegacyV2Field::OptionalU64(value) => {
                hasher.update([4]);
                match value {
                    Some(value) => {
                        hasher.update([1]);
                        hasher.update(value.to_be_bytes());
                    }
                    None => hasher.update([0]),
                }
            }
        }
    }
    format!("{V2_DIGEST_PREFIX}{}", hex::encode(hasher.finalize()))
}

fn legacy_decision_digest_v2(
    work_item_id: &str,
    decision: &str,
    reason: Option<&str>,
    expected_task_version: u64,
    actor_id: &str,
) -> String {
    legacy_digest_v2(
        "SUBMIT_DECISION",
        &[
            LegacyV2Field::Text(work_item_id),
            LegacyV2Field::Text(decision),
            LegacyV2Field::OptionalText(reason),
            LegacyV2Field::U64(expected_task_version),
            LegacyV2Field::Text(actor_id),
        ],
    )
}

fn legacy_cancel_blocked_digest_v2(
    blocker: &str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> String {
    legacy_digest_v2(
        "CANCEL_BLOCKED",
        &[
            LegacyV2Field::Text(blocker),
            LegacyV2Field::U64(expected_instance_version),
            LegacyV2Field::U64(expected_execution_version),
            LegacyV2Field::OptionalU64(expected_task_version),
            LegacyV2Field::Text(reason),
            LegacyV2Field::Text(actor_id),
        ],
    )
}

/// 按收据与本次摘要选择旧内存测试分支。
///
/// 新生产调用必须使用 [`PreparedCommandIdentity::classify`]；此函数只保留给
/// 不含 scope 查询的内存存储测试。
pub fn classify_receipt<'a>(
    receipt: Option<&'a ApprovalCommandReceipt>,
    payload_digest: &str,
) -> ReceiptBranch<'a> {
    let Some(receipt) = receipt else {
        return ReceiptBranch::Fresh;
    };
    match receipt.reconcile(payload_digest) {
        Ok(_) => ReceiptBranch::SamePayload(receipt),
        Err(_) => ReceiptBranch::PayloadConflict,
    }
}

/// 幂等冲突的稳定错误。
pub fn payload_conflict_error() -> Error {
    Error::from_approval_code(ErrorCode::ApprovalIdempotencyPayloadConflict)
}

/// 映射 receipt-first 第一笔写错误。
///
/// 仅审批命令收据 identity 唯一索引竞争允许退出失败会话后回读；收据主键、
/// 未知索引或其他业务集合唯一冲突均失败关闭。
pub fn map_receipt_first_write_error(error: database::Error) -> Error {
    if error.duplicate_index_name()
        == Some(database::repository::bpm::APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX)
    {
        Error::ReceiptDuplicate(error)
    } else {
        Error::from(error)
    }
}

/// 判断命令是否只允许在新会话有限回读原结果。
pub fn command_may_have_committed(error: &Error) -> bool {
    matches!(
        error,
        Error::OutcomeUnknown(_) | Error::ReceiptDuplicate(_) | Error::TransientTransaction(_)
    )
}

/// 返回命令结果有限回读的指数退避，上限为 160ms。
pub fn command_recovery_delay(attempt: usize) -> Duration {
    let shift = u32::try_from(attempt).unwrap_or(u32::MAX).min(5);
    Duration::from_millis(5_u64 << shift)
}

#[cfg(test)]
mod tests {
    use super::{
        cancel_blocked_identity, cancel_identity, decision_identity, document_cancel_identity,
        normalize_idempotency_key, resume_identity, start_identity, start_scope_candidates,
        upgrade_binding_identity, ReceiptBranch,
    };
    use bpm::ids::ApprovalCommandReceiptId;
    use bpm::model::{ApprovalCommandReceipt, Timestamp};

    fn key() -> bpm::model::IdempotencyKey {
        normalize_idempotency_key("  key-1  ").unwrap()
    }

    fn receipt(identity: &super::PreparedCommandIdentity) -> ApprovalCommandReceipt {
        ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            identity.current(),
            "result-1",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    fn assert_legacy_replay(identity: &super::PreparedCommandIdentity, scope: &str, digest: String) {
        let mut legacy = receipt(identity);
        legacy.scope_id = scope.to_string();
        legacy.payload_digest = digest;
        assert!(matches!(
            identity.classify(Some(&legacy)),
            ReceiptBranch::SamePayload(_)
        ));
    }

    #[test]
    fn execution_idempotency_key_is_canonical_before_lookup() {
        assert_eq!(key().as_str(), "key-1");
        assert!(normalize_idempotency_key("   ").is_err());
        assert!(normalize_idempotency_key(&"k".repeat(129)).is_err());
        assert!(normalize_idempotency_key(&"你".repeat(43)).is_err());
    }

    #[test]
    fn v3_identity_rejects_separator_null_optional_and_unicode_collisions() {
        let separator_left =
            decision_identity(key(), "exec-1", "a\u{1f}b", "APPROVE", Some("c"), 3, "u1").unwrap();
        let separator_right =
            decision_identity(key(), "exec-1", "a", "APPROVE", Some("b\u{1f}c"), 3, "u1").unwrap();
        assert_ne!(
            separator_left.current().digest(),
            separator_right.current().digest()
        );

        let none = decision_identity(key(), "exec-1", "wi-1", "APPROVE", None, 3, "用户").unwrap();
        let literal_null =
            decision_identity(key(), "exec-1", "wi-1", "APPROVE", Some("NULL"), 3, "用户").unwrap();
        let empty = decision_identity(key(), "exec-1", "wi-1", "APPROVE", Some(""), 3, "用户").unwrap();
        assert_ne!(none.current().digest(), literal_null.current().digest());
        assert_ne!(none.current().digest(), empty.current().digest());
        assert_ne!(literal_null.current().digest(), empty.current().digest());
    }

    #[test]
    fn current_v3_receipt_cannot_downgrade_to_legacy_digest() {
        let identity = decision_identity(key(), "exec-1", "wi-1", "APPROVE", Some("同意"), 3, "u1").unwrap();
        let mut receipt = receipt(&identity);
        receipt.payload_digest = super::legacy_decision_digest_v2("wi-1", "APPROVE", Some("同意"), 3, "u1");
        assert_eq!(identity.classify(Some(&receipt)), ReceiptBranch::PayloadConflict);
    }

    #[test]
    fn unknown_v3_identity_cannot_be_registered_as_legacy() {
        let rogue_scope = format!("v3:{}", "a".repeat(64));
        let rogue_digest = format!("v3:{}", "b".repeat(64));
        let identity = decision_identity(key(), "exec-1", "wi-1", "APPROVE", None, 3, "u1")
            .unwrap()
            .with_legacy(super::LegacyReceiptIdentity::exact(&rogue_scope, &rogue_digest));
        let mut receipt = receipt(&identity);
        receipt.scope_id = rogue_scope;
        receipt.payload_digest = rogue_digest;
        assert_eq!(identity.classify(Some(&receipt)), ReceiptBranch::PayloadConflict);
    }

    #[test]
    fn legacy_scope_and_digest_are_only_accepted_as_exact_pairs() {
        let identity = decision_identity(key(), "exec-1", "wi-1", "APPROVE", None, 3, "u1").unwrap();
        let mut receipt = receipt(&identity);
        receipt.scope_id = "exec-1".to_string();
        receipt.payload_digest = super::legacy_decision_digest_v2("wi-1", "APPROVE", None, 3, "u1");
        assert!(matches!(
            identity.classify(Some(&receipt)),
            ReceiptBranch::SamePayload(_)
        ));

        receipt.scope_id = identity.current().scope().as_str().to_string();
        assert_eq!(identity.classify(Some(&receipt)), ReceiptBranch::PayloadConflict);
    }

    #[test]
    fn known_legacy_execution_formats_remain_stable() {
        assert_eq!(
            super::legacy_start_scope("stock_adjustment", "STOCK_ADJUSTMENT", "adj-1", 3),
            "stock_adjustment\u{1f}STOCK_ADJUSTMENT\u{1f}adj-1\u{1f}3"
        );
        assert_eq!(
            super::legacy_start_digest("def-1", 7, 3, "u1"),
            "bc0d394485a1923f96cf171776c20eaa1c048cfb1849fe4c2972afc59e599202"
        );
        assert_eq!(
            super::legacy_decision_digest("wi-1", "REJECT", Some("资料不全"), 5, "u1"),
            "17eed8c1056c213f4ba7f4413ee94a96569c0ac7d28d9ddacbeb74466b386fe2"
        );
        assert_eq!(
            super::legacy_decision_digest_v2("wi-1", "REJECT", Some("资料不全"), 5, "u1"),
            "v2:033257875c9f5c66821f5806a7b0368294ea231a50e6835b1c3dcd3dfb679487"
        );
        assert_eq!(
            super::legacy_cancel_digest(3, 11, 13, Some(17), "撤回", "u1"),
            "c870d2579b4a9c9a017d8e30577b9c34b24f55a8e408a8ac21a473754c904f4a"
        );
        assert_eq!(
            super::legacy_document_cancel_digest(3, 7, 11, 13, Some(17), "撤回", "u1"),
            "acb3b94957c7c60ff44e8c54ef57d354ddb09061e8a05332253ebb0d7245954e"
        );
        assert_eq!(
            super::legacy_resume_digest(11, 13, 17, Some(19), "admin"),
            "bb6ce99d9f5c838095e8809fcf7567ada07a99b3af28287154e6c107385d053b"
        );
        assert_eq!(
            super::legacy_cancel_blocked_digest("GRAPH_CORRUPTED", 11, 13, None, "人工终止", "admin",),
            "18876a51ff88dc5bcd565918c78ed06240011f3cf5b56acedb745f354fe8da08"
        );
        assert_eq!(
            super::legacy_cancel_blocked_digest_v2("GRAPH_CORRUPTED", 11, 13, None, "人工终止", "admin",),
            "v2:8e3b49b55055600500771e00fd72e19630e49e4c3090e9e5ae967465d57ba9e8"
        );
    }

    #[test]
    fn each_known_legacy_writer_is_read_as_an_exact_pair() {
        let start = start_identity(
            key(),
            "stock_adjustment",
            "STOCK_ADJUSTMENT",
            "adj-1",
            3,
            "def-1",
            7,
            "u1",
        )
        .unwrap();
        assert_legacy_replay(
            &start,
            "stock_adjustment\u{1f}STOCK_ADJUSTMENT\u{1f}adj-1\u{1f}3",
            super::legacy_start_digest("def-1", 7, 3, "u1"),
        );

        let decision =
            decision_identity(key(), "exec-1", "wi-1", "REJECT", Some("资料不全"), 5, "u1").unwrap();
        assert_legacy_replay(
            &decision,
            "exec-1",
            super::legacy_decision_digest_v2("wi-1", "REJECT", Some("资料不全"), 5, "u1"),
        );
        assert_legacy_replay(
            &decision,
            "exec-1",
            super::legacy_decision_digest("wi-1", "REJECT", Some("资料不全"), 5, "u1"),
        );

        let cancel = cancel_identity(key(), "inst-1", 3, 11, 13, Some(17), "撤回", "u1").unwrap();
        assert_legacy_replay(
            &cancel,
            "inst-1",
            super::legacy_cancel_digest(3, 11, 13, Some(17), "撤回", "u1"),
        );

        let document_cancel =
            document_cancel_identity(key(), "inst-1", 3, 7, 11, 13, Some(17), "撤回", "u1").unwrap();
        assert_legacy_replay(
            &document_cancel,
            "inst-1",
            super::legacy_document_cancel_digest(3, 7, 11, 13, Some(17), "撤回", "u1"),
        );

        let resume = resume_identity(key(), "inst-1", 11, 13, 17, Some(19), "admin").unwrap();
        assert_legacy_replay(
            &resume,
            "inst-1",
            super::legacy_resume_digest(11, 13, 17, Some(19), "admin"),
        );

        let blocked = cancel_blocked_identity(
            key(),
            "inst-1",
            "GRAPH_CORRUPTED",
            11,
            13,
            None,
            "人工终止",
            "admin",
        )
        .unwrap();
        assert_legacy_replay(
            &blocked,
            "inst-1",
            super::legacy_cancel_blocked_digest_v2("GRAPH_CORRUPTED", 11, 13, None, "人工终止", "admin"),
        );
        assert_legacy_replay(
            &blocked,
            "inst-1",
            super::legacy_cancel_blocked_digest("GRAPH_CORRUPTED", 11, 13, None, "人工终止", "admin"),
        );
    }

    #[test]
    fn each_execution_command_has_stable_v3_golden_identity() {
        let start = start_identity(
            key(),
            "stock_adjustment",
            "STOCK_ADJUSTMENT",
            "adj-1",
            3,
            "def-1",
            7,
            "u1",
        )
        .unwrap();
        let decision =
            decision_identity(key(), "exec-1", "wi-1", "REJECT", Some("资料不全"), 5, "u1").unwrap();
        let cancel = cancel_identity(key(), "inst-1", 3, 11, 13, Some(17), "撤回", "u1").unwrap();
        let document_cancel =
            document_cancel_identity(key(), "inst-1", 3, 7, 11, 13, Some(17), "撤回", "u1").unwrap();
        let resume = resume_identity(key(), "inst-1", 11, 13, 17, Some(19), "admin").unwrap();
        let blocked = cancel_blocked_identity(
            key(),
            "inst-1",
            "GRAPH_CORRUPTED",
            11,
            13,
            None,
            "人工终止",
            "admin",
        )
        .unwrap();

        let golden = [
            (
                &start,
                "v3:dc6f7e4d641c5f83f44106a92d943a909406bbec32de038faba6000fba0317b2",
                "v3:cd91ac847e2bc94e7c4076c57db105101e441c3954e91466c514dd7bbe66ccae",
            ),
            (
                &decision,
                "v3:12c197973014a436361dbfdad4160d35c989bde80936e152c95cc1bec261f97a",
                "v3:fefd9fc191a939a7df752237e8deae3d130e96db5f3149282d18891464b4029e",
            ),
            (
                &cancel,
                "v3:b7989e68725984f3097bf2c816f7bb3bf41e6f4e8e7079ad79f9ae875fbfb29e",
                "v3:1c797a695e5b93e6237819b2d9af39240c1e44142450e4d72062db99eafd2a7b",
            ),
            (
                &document_cancel,
                "v3:0370f333bbef0dd4125e2cb1ade4baf7124898455b327f3540a0f6cf7bf9d1eb",
                "v3:6e7abef7185a4a237896dc84e8e11f7996c91d23b0620f6e557072ed7bf1c834",
            ),
            (
                &resume,
                "v3:31f9dd534335a8249ea08e93e6435cb557c92aa877b33a6ada418f6e30cd83a1",
                "v3:68522ea0bb336cd05f318deb7ff2d7167d75e4ce4ae3a68d8ba1f15a6f2fdc10",
            ),
            (
                &blocked,
                "v3:e3586c2ec45553303515c2e70a55aea67227e7190baf495ecf288a1397a84e37",
                "v3:73a859949131f064da4626e37e54705505947d948e67a55ec7411a779a79a066",
            ),
        ];
        for (identity, expected_scope, expected_digest) in golden {
            assert_eq!(identity.current().scope().as_str(), expected_scope);
            assert_eq!(identity.current().digest().as_str(), expected_digest);
        }
    }

    #[test]
    fn start_scope_candidates_pair_current_and_exact_legacy_scope() {
        let scopes = start_scope_candidates("stock_adjustment", "STOCK_ADJUSTMENT", "adj-1", 3).unwrap();
        assert_eq!(scopes.len(), 2);
        assert!(scopes[0].starts_with("v3:"));
        assert_eq!(
            scopes[1],
            "stock_adjustment\u{1f}STOCK_ADJUSTMENT\u{1f}adj-1\u{1f}3"
        );
    }

    #[test]
    fn upgrade_binding_is_v3_only_and_collision_free() {
        let exact = upgrade_binding_identity(
            "STOCK_ADJUSTMENT",
            "adj-1",
            7,
            3,
            "升级\u{1f}定义",
            "admin",
            key(),
        )
        .unwrap();
        let relocated = upgrade_binding_identity(
            "STOCK_ADJUSTMENT\u{1f}adj-1",
            "7",
            3,
            0,
            "升级",
            "定义\u{1f}admin",
            key(),
        )
        .unwrap();
        let literal_null =
            upgrade_binding_identity("STOCK_ADJUSTMENT", "adj-1", 7, 3, "NULL", "admin", key()).unwrap();
        let empty = upgrade_binding_identity("STOCK_ADJUSTMENT", "adj-1", 7, 3, "", "admin", key()).unwrap();

        assert_eq!(exact.scope_candidates().len(), 1);
        assert_ne!(exact.current().scope(), relocated.current().scope());
        assert_ne!(exact.current().digest(), relocated.current().digest());
        assert_ne!(literal_null.current().digest(), empty.current().digest());
        assert_eq!(
            exact.current().scope().as_str(),
            "v3:2151197d2b62a05fa8b3f2fe3c6d3e9a5c53db24a43af9efb1f68f584049d444"
        );
        assert_eq!(
            exact.current().digest().as_str(),
            "v3:29020cdbcf32812f24678520e8e94edf0feead816201baace007cd01c5c59731"
        );
    }
}
