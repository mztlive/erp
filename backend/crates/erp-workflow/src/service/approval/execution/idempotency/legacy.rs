//! 已知历史 writer 的精确 digest 与无前缀 scope。

use sha2::{Digest, Sha256};

const V2_DIGEST_PREFIX: &str = "v2:";

/// 对已经无碰撞编码的历史文本计算稳定 SHA-256 摘要。
///
/// 仅供精确历史 writer 或调用方自有的显式版本格式使用；当前命令必须通过
/// [`ApprovalCommandIdentity`] 写入 V3 摘要。
pub fn legacy_payload_digest(canonical: &str) -> String {
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

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

pub(super) fn legacy_start_scope(
    process_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    subject_version: u32,
) -> String {
    legacy_canonical_payload(&[process_kind, subject_kind, subject_id, &subject_version.to_string()])
}

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

fn push_length_prefixed(target: &mut String, value: &str) {
    target.push_str(&value.len().to_string());
    target.push(':');
    target.push_str(value);
}

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
