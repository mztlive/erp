//! 已知历史 writer 的精确 digest 与无前缀 scope。

use sha2::{Digest, Sha256};

const V2_DIGEST_PREFIX: &str = "v2:";

/// 对已经无碰撞编码的历史文本计算稳定 SHA-256 摘要。
///
/// 仅供精确历史 writer 或调用方自有的显式版本格式使用；当前命令必须通过
/// [`ApprovalCommandIdentity`] 写入 V3 摘要。
///
/// # 参数
/// * `canonical` - 已经按历史格式编码的文本。
///
/// # 返回
/// 返回十六进制 SHA-256，不带版本前缀。
///
/// # 错误
/// 不返回错误。
pub fn legacy_payload_digest(canonical: &str) -> String {
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

/// 空白字段记为 `NULL`，字段间用单元分隔符拼接，避免历史摘要碰撞。
fn legacy_canonical_payload(fields: &[&str]) -> String {
    fields
        .iter()
        .map(|field| {
            let trimmed = field.trim();
            if trimmed.is_empty() { "NULL" } else { trimmed }
        })
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

/// 形成历史启动收据的无前缀 scope。
///
/// # 参数
/// * `process_kind` - 流程种类。
/// * `subject_kind` - 主体种类。
/// * `subject_id` - 主体主键。
/// * `subject_version` - 冻结主体版本。
///
/// # 返回
/// 返回单元分隔符拼接的历史 scope。
///
/// # 错误
/// 不返回错误。
pub(super) fn legacy_start_scope(
    process_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    subject_version: u32,
) -> String {
    legacy_canonical_payload(&[process_kind, subject_kind, subject_id, &subject_version.to_string()])
}

/// 形成历史通用启动 writer 的无前缀 digest。
///
/// # 参数
/// * `binding_id` - 定义绑定 ID。
/// * `definition_version` - 定义版本。
/// * `subject_version` - 冻结主体版本。
/// * `actor_participant_id` - 启动人。
///
/// # 返回
/// 返回历史字段拼接后的 SHA-256。
///
/// # 错误
/// 不返回错误。
pub(super) fn legacy_start_digest(
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

/// 形成无前缀历史决定摘要。
///
/// # 参数
/// * `work_item_id` - 任务 ID。
/// * `decision` - 决定稳定码。
/// * `reason` - 决定原因；缺失时按空字段编码。
/// * `expected_task_version` - 期望任务版本。
/// * `actor_id` - 决定人。
///
/// # 返回
/// 返回历史字段拼接后的 SHA-256。
///
/// # 错误
/// 不返回错误。
pub(super) fn legacy_decision_digest(
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

/// 形成无前缀历史通用取消摘要。
///
/// # 参数
/// * `subject_version` - 冻结主体版本。
/// * `expected_instance_version` - 期望实例版本。
/// * `expected_execution_version` - 期望执行版本。
/// * `expected_task_version` - 期望任务版本；无任务时按空字段编码。
/// * `reason` - 取消原因。
/// * `actor_id` - 取消人。
///
/// # 返回
/// 返回历史字段拼接后的 SHA-256。
///
/// # 错误
/// 不返回错误。
pub(super) fn legacy_cancel_digest(
    subject_version: u32,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> String {
    let task_version = expected_task_version.map(|value| value.to_string()).unwrap_or_default();
    legacy_payload_digest(&legacy_canonical_payload(&[
        &subject_version.to_string(),
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &task_version,
        reason,
        actor_id,
    ]))
}

/// 形成历史业务单据撤回的长度前缀摘要。
///
/// # 参数
/// * `subject_version` - 冻结主体版本。
/// * `expected_document_version` - 期望单据版本。
/// * `expected_instance_version` - 期望实例版本。
/// * `expected_execution_version` - 期望执行版本。
/// * `expected_task_version` - 期望任务版本；`None` 编码为 `NONE`。
/// * `reason` - 撤回原因，写入前去掉首尾空白。
/// * `actor_id` - 撤回人，写入前去掉首尾空白。
///
/// # 返回
/// 返回长度前缀文本的 SHA-256。
///
/// # 错误
/// 不返回错误。
pub(super) fn legacy_document_cancel_digest(
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
        },
        None => push_length_prefixed(&mut canonical, "NONE"),
    }
    push_length_prefixed(&mut canonical, reason.trim());
    push_length_prefixed(&mut canonical, actor_id.trim());
    legacy_payload_digest(&canonical)
}

/// 按 `长度:内容` 追加字段，避免分隔符出现在值内时碰撞。
fn push_length_prefixed(target: &mut String, value: &str) {
    target.push_str(&value.len().to_string());
    target.push(':');
    target.push_str(value);
}

/// 形成无前缀历史原审批人恢复摘要。
///
/// # 参数
/// * `expected_instance_version` - 期望实例版本。
/// * `expected_execution_version` - 期望执行版本。
/// * `expected_assignment_version` - 期望绑定版本。
/// * `expected_closed_task_version` - 期望已关闭任务版本；无任务时按空字段编码。
/// * `actor_id` - 恢复人。
///
/// # 返回
/// 返回历史字段拼接后的 SHA-256。
///
/// # 错误
/// 不返回错误。
pub(super) fn legacy_resume_digest(
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_assignment_version: u64,
    expected_closed_task_version: Option<u64>,
    actor_id: &str,
) -> String {
    let task_version = expected_closed_task_version.map(|value| value.to_string()).unwrap_or_default();
    legacy_payload_digest(&legacy_canonical_payload(&[
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &expected_assignment_version.to_string(),
        &task_version,
        actor_id,
    ]))
}

/// 形成无前缀历史受阻取消摘要。
///
/// # 参数
/// * `blocker` - blocker 稳定码。
/// * `expected_instance_version` - 期望实例版本。
/// * `expected_execution_version` - 期望执行版本。
/// * `expected_task_version` - 期望任务版本；无任务时按空字段编码。
/// * `reason` - 取消原因。
/// * `actor_id` - 取消人。
///
/// # 返回
/// 返回历史字段拼接后的 SHA-256。
///
/// # 错误
/// 不返回错误。
pub(super) fn legacy_cancel_blocked_digest(
    blocker: &str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> String {
    let task_version = expected_task_version.map(|value| value.to_string()).unwrap_or_default();
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

/// 以域、版本和带类型标签的字段计算 `v2:` 前缀摘要。
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
            },
            LegacyV2Field::U64(value) => {
                hasher.update([2]);
                hasher.update(value.to_be_bytes());
            },
            LegacyV2Field::OptionalText(value) => {
                hasher.update([3]);
                match value {
                    Some(value) => {
                        hasher.update([1]);
                        update_text(&mut hasher, value);
                    },
                    None => hasher.update([0]),
                }
            },
            LegacyV2Field::OptionalU64(value) => {
                hasher.update([4]);
                match value {
                    Some(value) => {
                        hasher.update([1]);
                        hasher.update(value.to_be_bytes());
                    },
                    None => hasher.update([0]),
                }
            },
        }
    }
    format!("{V2_DIGEST_PREFIX}{}", hex::encode(hasher.finalize()))
}

/// 形成 V2 历史决定摘要。
///
/// # 参数
/// * `work_item_id` - 任务 ID。
/// * `decision` - 决定稳定码。
/// * `reason` - 决定原因；`None` 按可选空字段编码。
/// * `expected_task_version` - 期望任务版本。
/// * `actor_id` - 决定人。
///
/// # 返回
/// 返回 `v2:` 前缀的决定摘要。
///
/// # 错误
/// 不返回错误。
pub(super) fn legacy_decision_digest_v2(
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

/// 形成 V2 历史受阻取消摘要。
///
/// # 参数
/// * `blocker` - blocker 稳定码。
/// * `expected_instance_version` - 期望实例版本。
/// * `expected_execution_version` - 期望执行版本。
/// * `expected_task_version` - 期望任务版本；`None` 按可选空字段编码。
/// * `reason` - 取消原因。
/// * `actor_id` - 取消人。
///
/// # 返回
/// 返回 `v2:` 前缀的受阻取消摘要。
///
/// # 错误
/// 不返回错误。
pub(super) fn legacy_cancel_blocked_digest_v2(
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
