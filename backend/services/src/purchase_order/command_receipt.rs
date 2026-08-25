//! 采购单命令审计收据的稳定摘要与校验。

use entities::AuditLog;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{Error, Result};

const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";
const PURCHASE_ORDER_RESOURCE: &str = "purchase_order";

/// 生成不暴露原始幂等键的稳定采购命令收据 ID。
///
/// # 参数
/// * `prefix` - 便于运维识别命令类别的固定前缀
/// * `actor_id` - 已认证操作人 ID
/// * `action` - 服务端固定审计动作
/// * `purchase_order_id` - 采购单稳定 ID
/// * `idempotency_key` - 客户端原始幂等键
///
/// # 返回
/// 返回由操作人、动作、采购单与幂等键共同决定的稳定摘要 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 原始幂等键只参与不可逆摘要，不得出现在收据 ID 或审计消息中。
pub(super) fn command_receipt_id(
    prefix: &str,
    actor_id: &str,
    action: &str,
    purchase_order_id: &str,
    idempotency_key: &str,
) -> String {
    format!(
        "{prefix}{}",
        digest_parts(&[actor_id, action, purchase_order_id, idempotency_key])
    )
}

/// 构造采购命令请求载荷指纹。
///
/// # 参数
/// * `action` - 服务端固定审计动作
/// * `purchase_order_id` - 采购单稳定 ID
/// * `payload` - 已排除幂等键并按命令语义规范化的请求载荷
///
/// # 返回
/// 返回动作、采购单和请求载荷共同决定的 SHA-256 指纹。
///
/// # 错误
/// 请求载荷无法序列化时返回内部错误。
///
/// # 关键业务约束
/// 指纹不得包含原始幂等键，同键异载荷必须产生不同指纹。
pub(super) fn command_request_fingerprint<T: Serialize>(
    action: &str,
    purchase_order_id: &str,
    payload: &T,
) -> Result<String> {
    let payload = serde_json::to_string(payload)
        .map_err(|error| Error::Internal(format!("采购命令请求指纹序列化失败: {error}")))?;
    Ok(digest_parts(&[action, purchase_order_id, &payload]))
}

/// 编码可写入审计消息的采购命令收据。
///
/// # 参数
/// * `fingerprint` - 当前请求载荷指纹
/// * `receipt` - 需要稳定回放的原始命令结果
///
/// # 返回
/// 返回包含指纹和结果 JSON 的审计消息。
///
/// # 错误
/// 收据结果无法序列化时返回内部错误。
///
/// # 关键业务约束
/// 调用方提供的收据不得包含原始幂等键。
pub(super) fn command_receipt_message<T: Serialize>(fingerprint: &str, receipt: &T) -> Result<String> {
    let result = serde_json::to_string(receipt)
        .map_err(|error| Error::Internal(format!("采购命令幂等收据序列化失败: {error}")))?;
    Ok(format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={result}"
    ))
}

/// 校验审计身份、请求指纹并解析采购命令收据。
///
/// # 参数
/// * `audit` - 按稳定收据 ID 读取的审计日志
/// * `expected_actor_id` - 当前已认证操作人 ID
/// * `expected_action` - 当前命令固定审计动作
/// * `expected_purchase_order_id` - 当前路径采购单 ID
/// * `expected_fingerprint` - 当前请求载荷指纹
///
/// # 返回
/// 身份与指纹全部一致时返回原命令结果载荷。
///
/// # 错误
/// 收据身份不一致或同键异载荷时返回 409；收据格式损坏时返回内部错误。
///
/// # 关键业务约束
/// 必须先比较指纹，再读取结果 JSON，避免同键异载荷回放旧结果。
pub(super) fn parse_command_receipt<T: DeserializeOwned>(
    audit: &AuditLog,
    expected_actor_id: &str,
    expected_action: &str,
    expected_purchase_order_id: &str,
    expected_fingerprint: &str,
) -> Result<T> {
    ensure_receipt_identity(
        audit,
        expected_actor_id,
        expected_action,
        expected_purchase_order_id,
    )?;
    let result = receipt_result_json(audit.message.as_deref().unwrap_or_default(), expected_fingerprint)?;
    serde_json::from_str(result)
        .map_err(|error| Error::Internal(format!("采购命令幂等收据结果非法: {error}")))
}

/// 校验采购命令收据的审计身份与目标资源。
///
/// # 参数
/// * `audit` - 按稳定收据 ID 读取的审计日志
/// * `expected_actor_id` - 当前已认证操作人 ID
/// * `expected_action` - 当前命令固定审计动作
/// * `expected_purchase_order_id` - 当前路径采购单 ID
///
/// # 返回
/// 审计身份和资源完全一致时返回 `Ok(())`。
///
/// # 错误
/// 收据被其他操作人、动作或采购单占用时返回 409。
///
/// # 关键业务约束
/// 不接受失败审计或非采购单资源作为成功命令收据。
fn ensure_receipt_identity(
    audit: &AuditLog,
    expected_actor_id: &str,
    expected_action: &str,
    expected_purchase_order_id: &str,
) -> Result<()> {
    let matches = audit.success
        && audit.actor_id == expected_actor_id
        && audit.action == expected_action
        && audit.resource_type == PURCHASE_ORDER_RESOURCE
        && audit.resource_id.as_deref() == Some(expected_purchase_order_id);
    if !matches {
        return Err(idempotency_conflict());
    }
    Ok(())
}

/// 从审计消息提取并校验请求指纹和结果 JSON。
///
/// # 参数
/// * `message` - 审计日志消息
/// * `expected_fingerprint` - 当前请求载荷指纹
///
/// # 返回
/// 指纹一致时返回尚未反序列化的结果 JSON。
///
/// # 错误
/// 消息格式损坏时返回内部错误，同键异载荷时返回 409。
///
/// # 关键业务约束
/// 结果 JSON 只能在指纹匹配后交给调用方解析。
fn receipt_result_json<'a>(message: &'a str, expected_fingerprint: &str) -> Result<&'a str> {
    let (fingerprint, result) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal("采购命令幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(idempotency_conflict());
    }
    Ok(result)
}

/// 返回统一的采购命令幂等键载荷冲突。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回 HTTP 409 对应的稳定服务错误。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 错误消息不得泄露原始幂等键或历史请求载荷。
fn idempotency_conflict() -> Error {
    Error::ConflictError("幂等键已用于不同采购命令".to_string())
}

/// 对带长度边界的文本片段计算稳定 SHA-256 摘要。
///
/// # 参数
/// * `parts` - 按业务定义顺序排列的文本片段
///
/// # 返回
/// 返回 64 位小写十六进制摘要。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 每段使用固定宽度长度前缀，避免简单拼接产生歧义。
fn digest_parts(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use entities::{AccountKind, AuditLog, AuditLogData};
    use serde::{Deserialize, Serialize};

    use super::{
        command_receipt_id, command_receipt_message, command_request_fingerprint, parse_command_receipt,
    };
    use crate::errors::Error;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct TestReceipt {
        value: String,
    }

    /// 构造最小有效采购命令审计日志。
    ///
    /// # 参数
    /// * `message` - 已编码命令收据消息
    ///
    /// # 返回
    /// 返回用于纯函数校验的审计实体。
    ///
    /// # 错误
    /// 测试数据固定有效，不返回错误。
    fn audit_fixture(message: String) -> AuditLog {
        AuditLog::new(
            "receipt-1".to_string(),
            AuditLogData {
                actor_id: "actor-1".to_string(),
                actor_account: "buyer".to_string(),
                actor_type: AccountKind::Admin,
                action: "purchase_order.update".to_string(),
                resource_type: "purchase_order".to_string(),
                resource_id: Some("po-1".to_string()),
                success: true,
                message: Some(message),
            },
        )
        .expect("audit fixture should be valid")
    }

    /// 验证稳定收据 ID 覆盖全部命令身份且不泄露原始键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 任一命令身份未进入摘要或原键泄露时测试失败。
    #[test]
    fn receipt_id_is_stable_partitioned_and_key_safe() {
        let key = "raw-secret-idempotency-key";
        let receipt = command_receipt_id(
            "purchase-order-command-",
            "actor-1",
            "purchase_order.update",
            "po-1",
            key,
        );

        assert_eq!(
            receipt,
            command_receipt_id(
                "purchase-order-command-",
                "actor-1",
                "purchase_order.update",
                "po-1",
                key,
            )
        );
        assert!(!receipt.contains(key));
        assert_ne!(
            receipt,
            command_receipt_id(
                "purchase-order-command-",
                "actor-2",
                "purchase_order.update",
                "po-1",
                key,
            )
        );
        assert_ne!(
            receipt,
            command_receipt_id(
                "purchase-order-command-",
                "actor-1",
                "purchase_order.void",
                "po-1",
                key,
            )
        );
        assert_ne!(
            receipt,
            command_receipt_id(
                "purchase-order-command-",
                "actor-1",
                "purchase_order.update",
                "po-2",
                key,
            )
        );
        assert_ne!(
            receipt,
            command_receipt_id(
                "purchase-order-command-",
                "actor-1",
                "purchase_order.update",
                "po-1",
                "another-key",
            )
        );
    }

    /// 验证同指纹回放原结果而异指纹返回 409。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 收据结果不能稳定回放或异载荷未冲突时测试失败。
    #[test]
    fn receipt_replays_same_payload_and_rejects_different_payload() {
        let fingerprint =
            command_request_fingerprint("purchase_order.update", "po-1", &(1_u64, "payload-a")).unwrap();
        let message = command_receipt_message(
            &fingerprint,
            &TestReceipt {
                value: "original".to_string(),
            },
        )
        .unwrap();
        let audit = audit_fixture(message);

        let replayed: TestReceipt =
            parse_command_receipt(&audit, "actor-1", "purchase_order.update", "po-1", &fingerprint).unwrap();
        assert_eq!(replayed.value, "original");

        let different =
            command_request_fingerprint("purchase_order.update", "po-1", &(1_u64, "payload-b")).unwrap();
        assert!(matches!(
            parse_command_receipt::<TestReceipt>(
                &audit,
                "actor-1",
                "purchase_order.update",
                "po-1",
                &different,
            ),
            Err(Error::ConflictError(_))
        ));
    }
}
