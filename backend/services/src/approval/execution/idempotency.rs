//! 审批命令幂等摘要与收据分支。

use bpm::model::types::{DIGEST_MAX_LEN, SCOPE_MAX_LEN};
use bpm::model::ApprovalCommandReceipt;
use sha2::{Digest, Sha256};

use crate::errors::{Error, Result};

/// 幂等键规范化后的最大长度。
const IDEMPOTENCY_KEY_MAX_LEN: usize = SCOPE_MAX_LEN;

/// 收据查找后的分支。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptBranch<'a> {
    /// 收据不存在，允许执行命令。
    Fresh,
    /// 同载荷，只允许授权回读。
    SamePayload(&'a ApprovalCommandReceipt),
    /// 异载荷冲突。
    PayloadConflict,
}

/// 规范化幂等键：去空白、非空、限制长度。
///
/// # 参数
/// * `raw` - 调用方提交的幂等键
///
/// # 错误
/// 空值或超长时返回校验错误。
pub fn normalize_idempotency_key(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError("幂等键不能为空".to_string()));
    }
    if trimmed.len() > IDEMPOTENCY_KEY_MAX_LEN {
        return Err(Error::ValidationError("幂等键过长".to_string()));
    }
    Ok(trimmed.to_string())
}

/// 按固定字段顺序编码 canonical 文本。空值写为 `NULL`。
///
/// # 参数
/// * `fields` - 已按合同顺序排列的字段
///
/// # 返回
/// 返回 UTF-8 canonical 字符串。
pub fn canonical_payload(fields: &[&str]) -> String {
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

/// 对 canonical 文本计算稳定摘要。
///
/// # 参数
/// * `canonical` - 已按固定字段顺序编码的文本
///
/// # 返回
/// 返回不超过摘要长度上限的十六进制摘要。
pub fn payload_digest(canonical: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(canonical.as_bytes()));
    if digest.len() <= DIGEST_MAX_LEN {
        digest
    } else {
        digest[..DIGEST_MAX_LEN].to_string()
    }
}

/// 启动命令的作用域。
///
/// # 参数
/// * `process_kind` - 流程种类稳定代码
/// * `subject_kind` - 业务对象种类
/// * `subject_id` - 业务对象主键
/// * `subject_version` - 冻结提交版本
///
/// # 返回
/// 返回启动命令 scope。
pub fn start_scope(process_kind: &str, subject_kind: &str, subject_id: &str, subject_version: u32) -> String {
    canonical_payload(&[
        process_kind,
        subject_kind,
        subject_id,
        &subject_version.to_string(),
    ])
}

/// 启动命令 canonical 载荷。
///
/// # 参数
/// * `binding_id` - 绑定定义 ID
/// * `definition_version` - 定义版本
/// * `subject_version` - 提交版本
/// * `actor_participant_id` - 启动人
///
/// # 返回
/// 返回启动载荷摘要。
pub fn start_digest(
    binding_id: &str,
    definition_version: u32,
    subject_version: u32,
    actor_participant_id: &str,
) -> String {
    payload_digest(&canonical_payload(&[
        binding_id,
        &definition_version.to_string(),
        &subject_version.to_string(),
        actor_participant_id,
    ]))
}

/// 决定命令 canonical 载荷。
///
/// # 参数
/// * `work_item_id` - 任务 ID
/// * `decision` - `APPROVE` 或 `REJECT`
/// * `reason` - 已 trim 原因，可空
/// * `expected_task_version` - 期望任务版本
/// * `actor_id` - 决定人
///
/// # 返回
/// 返回决定载荷摘要。
pub fn decision_digest(
    work_item_id: &str,
    decision: &str,
    reason: Option<&str>,
    expected_task_version: u64,
    actor_id: &str,
) -> String {
    payload_digest(&canonical_payload(&[
        work_item_id,
        decision,
        reason.unwrap_or(""),
        &expected_task_version.to_string(),
        actor_id,
    ]))
}

/// 取消命令 canonical 载荷。
///
/// # 参数
/// * `subject_version` - 提交版本
/// * `expected_instance_version` - 期望实例版本
/// * `expected_execution_version` - 期望执行版本
/// * `expected_task_version` - 可空任务版本
/// * `reason` - 已 trim 原因
/// * `actor_id` - 取消人
///
/// # 返回
/// 返回取消载荷摘要。
pub fn cancel_digest(
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
    payload_digest(&canonical_payload(&[
        &subject_version.to_string(),
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &task_version,
        reason,
        actor_id,
    ]))
}

/// 恢复命令 canonical 载荷。
///
/// # 参数
/// * `expected_instance_version` - 期望实例版本
/// * `expected_execution_version` - 期望执行版本
/// * `expected_assignment_version` - 期望绑定版本
/// * `expected_closed_task_version` - 可空已关闭任务版本
/// * `actor_id` - 恢复人
///
/// # 返回
/// 返回恢复载荷摘要。
pub fn resume_digest(
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_assignment_version: u64,
    expected_closed_task_version: Option<u64>,
    actor_id: &str,
) -> String {
    let task_version = expected_closed_task_version
        .map(|value| value.to_string())
        .unwrap_or_default();
    payload_digest(&canonical_payload(&[
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &expected_assignment_version.to_string(),
        &task_version,
        actor_id,
    ]))
}

/// 改派命令 canonical 载荷。
///
/// # 参数
/// * `target_user` - 目标用户
/// * `expected_instance_version` - 期望实例版本
/// * `expected_execution_version` - 期望执行版本
/// * `expected_assignment_version` - 期望绑定版本
/// * `expected_task_version` - 可空任务版本
/// * `reason` - 已 trim 原因
/// * `actor_id` - 改派人
///
/// # 返回
/// 返回改派载荷摘要。
pub fn reassign_digest(
    target_user: &str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_assignment_version: u64,
    expected_task_version: Option<u64>,
    reason: &str,
    actor_id: &str,
) -> String {
    let task_version = expected_task_version
        .map(|value| value.to_string())
        .unwrap_or_default();
    payload_digest(&canonical_payload(&[
        target_user,
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &expected_assignment_version.to_string(),
        &task_version,
        reason,
        actor_id,
    ]))
}

/// 受阻取消 canonical 载荷。
///
/// # 参数
/// * `blocker` - 当前 blocker
/// * `expected_instance_version` - 期望实例版本
/// * `expected_execution_version` - 期望执行版本
/// * `expected_task_version` - 可空任务版本
/// * `reason` - 已 trim 原因
/// * `actor_id` - 取消人
///
/// # 返回
/// 返回受阻取消载荷摘要。
pub fn cancel_blocked_digest(
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
    payload_digest(&canonical_payload(&[
        blocker,
        &expected_instance_version.to_string(),
        &expected_execution_version.to_string(),
        &task_version,
        reason,
        actor_id,
    ]))
}

/// 按收据与本次摘要选择幂等分支。
///
/// # 参数
/// * `receipt` - 已读取收据
/// * `payload_digest` - 本次请求摘要
///
/// # 返回
/// 返回新鲜执行、同载荷回读或异载荷冲突。
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
///
/// # 返回
/// 返回 `APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT`。
pub fn payload_conflict_error() -> Error {
    Error::ConflictError("APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        cancel_digest, canonical_payload, classify_receipt, decision_digest, normalize_idempotency_key,
        payload_conflict_error, payload_digest, resume_digest, start_digest, start_scope, ReceiptBranch,
    };
    use bpm::ids::ApprovalCommandReceiptId;
    use bpm::model::types::ApprovalCommandKind;
    use bpm::model::{ApprovalCommandReceipt, Timestamp};

    /// 幂等键必须 trim、非空并限制长度。
    #[test]
    fn execution_idempotency_key_is_normalized() {
        assert_eq!(normalize_idempotency_key("  key-1  ").unwrap(), "key-1");
        assert!(normalize_idempotency_key("   ").is_err());
        assert!(normalize_idempotency_key(&"k".repeat(129)).is_err());
    }

    /// canonical 编码使用固定分隔符和 NULL 空值。
    #[test]
    fn execution_canonical_hash_is_stable() {
        let first = payload_digest(&canonical_payload(&["wi-1", "APPROVE", "", "3", "u1"]));
        let second = payload_digest(&canonical_payload(&["wi-1", "APPROVE", "", "3", "u1"]));
        assert_eq!(first, second);
        assert_ne!(
            first,
            payload_digest(&canonical_payload(&["wi-1", "REJECT", "", "3", "u1"]))
        );
        assert!(canonical_payload(&["", "x"]).starts_with("NULL"));
    }

    /// 同载荷回读，异载荷冲突。
    #[test]
    fn execution_receipt_same_payload_replays() {
        let digest = decision_digest("wi-1", "APPROVE", None, 3, "u1");
        let receipt = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            ApprovalCommandKind::SubmitDecision,
            "exec-1",
            "key-1",
            digest.clone(),
            "exec-1",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            classify_receipt(Some(&receipt), &digest),
            ReceiptBranch::SamePayload(_)
        ));
        assert_eq!(
            classify_receipt(Some(&receipt), "other"),
            ReceiptBranch::PayloadConflict
        );
        assert_eq!(classify_receipt(None, &digest), ReceiptBranch::Fresh);
        assert_eq!(
            payload_conflict_error().to_string(),
            "数据冲突: APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT"
        );
    }

    /// 启动 scope 与各类 digest 对字段顺序敏感。
    #[test]
    fn execution_command_digests_include_ordered_fields() {
        assert_ne!(
            start_scope("stock_adjustment", "stock_adjustment", "adj-1", 1),
            start_scope("stock_adjustment", "stock_adjustment", "adj-1", 2)
        );
        assert_ne!(
            start_digest("def-1", 1, 1, "u1"),
            start_digest("def-1", 2, 1, "u1")
        );
        assert_ne!(
            cancel_digest(1, 1, 1, None, "撤回", "u1"),
            cancel_digest(1, 1, 1, Some(2), "撤回", "u1")
        );
        assert_ne!(
            resume_digest(1, 1, 1, None, "admin"),
            resume_digest(1, 1, 2, None, "admin")
        );
    }
}
