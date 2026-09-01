//! 统一采购命令收据值对象：稳定身份、请求指纹与 wire 编解码。
//!
//! 采购命令（作废、依据创建、选源创建、提交）把幂等收据写入 `audit_logs` 消息，
//! 本模块集中收据 ID 摘要、请求载荷指纹、`command_sha256=` 消息编码/解码、
//! 目标身份校验与指纹校验；Service 只读取审计事实、执行授权与事务并映射响应。
//! 摘要与消息形态必须保持历史兼容，任何变化都会破坏存量收据回放。

use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

use crate::audit_log::AuditLog;
use crate::errors::{Error, Result};

/// 收据消息的前缀；历史持久化形态，禁止变更。
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";
/// 采购命令收据固定的资源类型。
const PURCHASE_ORDER_RESOURCE: &str = "purchase_order";

/// 历史收据 ID 形态；只影响存量兼容查询候选，新写入一律使用长度前缀规范摘要。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyReceiptIdScheme {
    /// 无历史形态；新写入与历史写入均使用长度前缀规范摘要。
    None,
    /// 历史 `{prefix}{sha256("{actor}|{action}|{target}|{key}")}` 整串摘要。
    ///
    /// 采购提交路径的存量收据使用整串摘要形态；新写入使用规范摘要，并保留该
    /// 历史 ID 作为只读查询候选。
    WholeStringJoined,
}

/// 采购命令收据的稳定身份。
///
/// 收据 ID 由操作人、动作、目标（可选）与原始幂等键共同决定，原始幂等键只参与
/// 不可逆摘要，绝不进入 ID 明文；`id_candidates` 按当前优先、历史其次排序，
/// 供 Service 逐候选回读存量收据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseCommandReceiptIdentity {
    receipt_id: String,
    legacy_receipt_ids: Vec<String>,
}

impl PurchaseCommandReceiptIdentity {
    /// 返回新写入使用的收据 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `{prefix}{64hex}` 形式的稳定收据 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 相同身份输入必须产生相同 ID；ID 不得包含原始幂等键。
    pub fn receipt_id(&self) -> &str {
        &self.receipt_id
    }

    /// 返回收据查询候选：当前 ID 优先，历史兼容 ID 其次。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回按当前优先、历史其次排序的候选 ID 列表。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 调用方必须逐个候选回读审计事实；命中任意候选后按身份与指纹校验。
    pub fn id_candidates(&self) -> Vec<&str> {
        std::iter::once(self.receipt_id.as_str())
            .chain(self.legacy_receipt_ids.iter().map(String::as_str))
            .collect()
    }
}

/// 采购命令收据解码失败分类。
///
/// Service 依据分类映射 HTTP 语义：身份不一致与同键异载荷为冲突（部分路径按
/// 历史行为映射为内部错误），形态损坏为内部错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurchaseCommandReceiptError {
    /// 审计事实的操作人、动作、资源类型或目标与当前命令不一致。
    IdentityMismatch,
    /// 同一收据 ID 已被不同请求载荷占用。
    PayloadConflict,
    /// 收据形态损坏或结果载荷无法解析。
    Corrupted(String),
}

/// 采购命令收据结果载荷的持久化 wire 编解码。
///
/// 标准结果类型直接使用 JSON 序列化（[`Serialize`] + [`DeserializeOwned`] 自动
/// 实现）；存在历史字段形态的结果类型（如采购提交的管道分隔格式）必须手动实现
/// 本 trait 并保持存量格式可解码。
pub trait PurchaseReceiptWire: Sized {
    /// 把结果编码为可持久化的 wire 文本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回规范化 wire 文本。
    ///
    /// # 错误
    /// 结果无法序列化时返回错误。
    ///
    /// # 关键业务约束
    /// 编码必须确定且不包含原始幂等键。
    fn encode_wire(&self) -> Result<String>;

    /// 从持久化 wire 文本解码结果。
    ///
    /// # 参数
    /// * `wire` - 已通过指纹校验的结果文本
    ///
    /// # 返回
    /// 形态可识别时返回结果；无法识别时返回 `None`。
    ///
    /// # 关键业务约束
    /// 解码失败不得 panic，统一由调用方映射为内部错误。
    fn decode_wire(wire: &str) -> Option<Self>;
}

impl<T: Serialize + DeserializeOwned> PurchaseReceiptWire for T {
    fn encode_wire(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|error| Error::from(format!("采购命令收据结果序列化失败: {error}")))
    }

    fn decode_wire(wire: &str) -> Option<Self> {
        serde_json::from_str(wire).ok()
    }
}

/// 统一采购命令收据值对象：请求指纹与命令结果载荷。
///
/// `T` 为命令结果载荷；`identity` 负责稳定收据 ID，`encode_message`/`decode`
/// 负责 `command_sha256=` 消息的编码、解码、目标身份校验与指纹校验。本类型
/// 无 I/O、无全局时钟、无密钥；摘要只使用不可逆 SHA-256。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseCommandReceipt<T> {
    fingerprint: String,
    result: T,
}

impl<T> PurchaseCommandReceipt<T> {
    /// 形成稳定收据身份（新写入 ID 与历史兼容候选）。
    ///
    /// # 参数
    /// * `prefix` - 便于运维识别命令类别的固定前缀
    /// * `actor_id` - 已认证操作人 ID
    /// * `action` - 服务端固定审计动作
    /// * `target_id` - 命令目标资源 ID；目标在命令执行后才存在（依据创建）时传 `None`
    /// * `idempotency_key` - 客户端原始幂等键
    /// * `legacy` - 存量收据的历史 ID 形态
    ///
    /// # 返回
    /// 返回携带新写入 ID 与历史查询候选的稳定身份。
    ///
    /// # 错误
    /// 前缀、操作人、动作、目标或幂等键为空时返回校验错误。
    ///
    /// # 关键业务约束
    /// 原始幂等键只参与不可逆摘要；身份部分变化（操作人、动作、目标、键任一
    /// 不同）必须产生不同 ID；新写入一律使用长度前缀规范摘要。
    pub fn identity(
        prefix: &str,
        actor_id: &str,
        action: &str,
        target_id: Option<&str>,
        idempotency_key: &str,
        legacy: LegacyReceiptIdScheme,
    ) -> Result<PurchaseCommandReceiptIdentity> {
        if prefix.trim().is_empty() {
            return Err(Error::from("命令收据前缀不能为空"));
        }
        if actor_id.trim().is_empty() {
            return Err(Error::from("命令收据操作人不能为空"));
        }
        if action.trim().is_empty() {
            return Err(Error::from("命令收据动作不能为空"));
        }
        if idempotency_key.trim().is_empty() {
            return Err(Error::from("命令收据幂等键不能为空"));
        }
        let mut parts = vec![actor_id.to_string(), action.to_string()];
        if let Some(target_id) = target_id {
            if target_id.trim().is_empty() {
                return Err(Error::from("命令收据目标 ID 不能为空"));
            }
            parts.push(target_id.to_string());
        }
        parts.push(idempotency_key.to_string());
        let mut legacy_receipt_ids = Vec::new();
        if legacy == LegacyReceiptIdScheme::WholeStringJoined {
            let target_id = target_id.ok_or_else(|| Error::from("整串摘要收据身份必须携带目标 ID"))?;
            let joined = format!("{actor_id}|{action}|{target_id}|{idempotency_key}");
            legacy_receipt_ids.push(format!(
                "{prefix}{}",
                hex::encode(Sha256::digest(joined.as_bytes()))
            ));
        }
        Ok(PurchaseCommandReceiptIdentity {
            receipt_id: format!("{prefix}{}", digest_parts(parts)),
            legacy_receipt_ids,
        })
    }

    /// 构造携带结果载荷的收据。
    ///
    /// # 参数
    /// * `fingerprint` - 已排除幂等键的当前请求载荷指纹
    /// * `result` - 首次成功执行的命令结果
    ///
    /// # 返回
    /// 返回可编码为审计消息的收据。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 结果不得包含原始幂等键；指纹必须由本模块摘要函数生成。
    pub fn new(fingerprint: impl Into<String>, result: T) -> Self {
        Self {
            fingerprint: fingerprint.into(),
            result,
        }
    }

    /// 返回当前请求载荷指纹。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 64 位小写十六进制摘要。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 指纹不得包含原始幂等键。
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// 返回命令结果载荷引用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回首次成功执行的结果。
    ///
    /// # 错误
    /// 无。
    pub fn payload(&self) -> &T {
        &self.result
    }

    /// 解包命令结果载荷。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回首次成功执行的结果。
    ///
    /// # 错误
    /// 无。
    pub fn into_payload(self) -> T {
        self.result
    }
}

impl<T: PurchaseReceiptWire> PurchaseCommandReceipt<T> {
    /// 编码可写入审计消息的收据。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `command_sha256={fingerprint};result={wire}` 形态的审计消息。
    ///
    /// # 错误
    /// 结果载荷无法序列化时返回内部错误。
    ///
    /// # 关键业务约束
    /// 消息形态必须与存量收据一致，保证历史消息可被 `decode` 回放。
    pub fn encode_message(&self) -> Result<String> {
        let result = self.result.encode_wire()?;
        Ok(format!(
            "{COMMAND_FINGERPRINT_PREFIX}{};result={result}",
            self.fingerprint
        ))
    }

    /// 校验审计身份、请求指纹并解码采购命令收据。
    ///
    /// # 参数
    /// * `audit` - 按稳定收据 ID 读取的审计日志
    /// * `expected_actor_id` - 当前已认证操作人 ID
    /// * `expected_action` - 当前命令固定审计动作
    /// * `expected_target_id` - 当前命令目标资源 ID；目标执行后才存在时传 `None`
    /// * `expected_fingerprint` - 当前请求载荷指纹
    ///
    /// # 返回
    /// 身份与指纹全部一致时返回已解码收据。
    ///
    /// # 错误
    /// 身份不一致返回 [`PurchaseCommandReceiptError::IdentityMismatch`]；同键
    /// 异载荷返回 [`PurchaseCommandReceiptError::PayloadConflict`]；消息或结果
    /// 形态损坏返回 [`PurchaseCommandReceiptError::Corrupted`]。
    ///
    /// # 关键业务约束
    /// 必须先校验身份，再比较指纹，最后解码结果；指纹不一致时不得读取结果载荷，
    /// 避免同键异载荷回放旧结果。
    pub fn decode(
        audit: &AuditLog,
        expected_actor_id: &str,
        expected_action: &str,
        expected_target_id: Option<&str>,
        expected_fingerprint: &str,
    ) -> std::result::Result<Self, PurchaseCommandReceiptError> {
        if !audit.success
            || audit.actor_id != expected_actor_id
            || audit.action != expected_action
            || audit.resource_type != PURCHASE_ORDER_RESOURCE
        {
            return Err(PurchaseCommandReceiptError::IdentityMismatch);
        }
        if let Some(expected_target_id) = expected_target_id {
            if audit.resource_id.as_deref() != Some(expected_target_id) {
                return Err(PurchaseCommandReceiptError::IdentityMismatch);
            }
        }
        let message = audit
            .message
            .as_deref()
            .ok_or_else(|| PurchaseCommandReceiptError::Corrupted("采购命令幂等收据缺少结果".to_string()))?;
        let (fingerprint, result) = message
            .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
            .and_then(|value| value.split_once(";result="))
            .ok_or_else(|| PurchaseCommandReceiptError::Corrupted("采购命令幂等收据格式非法".to_string()))?;
        if fingerprint != expected_fingerprint {
            return Err(PurchaseCommandReceiptError::PayloadConflict);
        }
        let result = T::decode_wire(result)
            .ok_or_else(|| PurchaseCommandReceiptError::Corrupted("采购命令幂等收据结果非法".to_string()))?;
        Ok(Self {
            fingerprint: fingerprint.to_string(),
            result,
        })
    }
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
/// 每段使用固定宽度长度前缀，避免简单拼接产生歧义；摘要算法与存量指纹一致，
/// 修改会破坏幂等兼容。
pub fn digest_parts<I, S>(parts: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut hasher = Sha256::new();
    for part in parts {
        let part = part.as_ref();
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// 构造动作、目标与请求载荷共同决定的命令请求指纹。
///
/// # 参数
/// * `action` - 服务端固定审计动作
/// * `target_id` - 命令目标资源 ID
/// * `payload` - 已排除幂等键并按命令语义规范化的请求载荷
///
/// # 返回
/// 返回动作、目标和请求载荷共同决定的 SHA-256 指纹。
///
/// # 错误
/// 请求载荷无法序列化时返回内部错误。
///
/// # 关键业务约束
/// 指纹不得包含原始幂等键；同键异载荷必须产生不同指纹；载荷 JSON 序列化
/// 形态必须与存量指纹一致。
pub fn payload_fingerprint<T: Serialize>(action: &str, target_id: &str, payload: &T) -> Result<String> {
    let payload = serde_json::to_string(payload)
        .map_err(|error| Error::from(format!("采购命令请求指纹序列化失败: {error}")))?;
    Ok(digest_parts([action, target_id, payload.as_str()]))
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};

    use super::{
        digest_parts, payload_fingerprint, LegacyReceiptIdScheme, PurchaseCommandReceipt,
        PurchaseCommandReceiptError, PurchaseReceiptWire,
    };
    use crate::audit_log::{AuditLog, AuditLogData};
    use crate::errors::Result;
    use crate::AccountKind;

    /// 标准 JSON 形态的测试结果载荷。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestReceipt {
        purchase_order_id: String,
        lock_version: u64,
    }

    /// 测试请求载荷，包含不应进入摘要明文的原始幂等键。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestFingerprintPayload {
        idempotency_key: String,
        reason: String,
    }

    /// 历史管道分隔形态的测试结果载荷（模拟采购提交存量 wire）。
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct PipeReceipt {
        purchase_no: String,
        lock_version: u64,
    }

    impl PurchaseReceiptWire for PipeReceipt {
        fn encode_wire(&self) -> Result<String> {
            Ok(format!("{}|{}", self.purchase_no, self.lock_version))
        }

        fn decode_wire(wire: &str) -> Option<Self> {
            let mut fields = wire.split('|');
            let purchase_no = fields.next()?.to_string();
            let lock_version = fields.next()?.parse().ok()?;
            if fields.next().is_some() {
                return None;
            }
            Some(Self {
                purchase_no,
                lock_version,
            })
        }
    }

    /// 构造最小有效采购命令审计数据。
    ///
    /// # 参数
    /// * `message` - 已编码命令收据消息
    ///
    /// # 返回
    /// 返回用于纯函数校验的审计数据。
    ///
    /// # 错误
    /// 测试数据固定有效，不返回错误。
    fn audit_data(message: Option<String>) -> AuditLogData {
        AuditLogData {
            actor_id: "actor-1".to_string(),
            actor_account: "buyer".to_string(),
            actor_type: AccountKind::Admin,
            action: "purchase_order.update".to_string(),
            resource_type: "purchase_order".to_string(),
            resource_id: Some("po-1".to_string()),
            success: true,
            message,
        }
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
        AuditLog::new("receipt-1".to_string(), audit_data(Some(message))).expect("audit fixture 必须合法")
    }

    /// 验证摘要带长度前缀，避免简单拼接歧义。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 不同切分产生相同摘要时测试失败。
    #[test]
    fn digest_parts_is_length_prefixed_and_stable() {
        assert_ne!(
            digest_parts(["ab".to_string(), "c".to_string()]),
            digest_parts(["a".to_string(), "bc".to_string()])
        );
        assert_eq!(
            digest_parts(["ab".to_string(), "c".to_string()]),
            digest_parts(["ab".to_string(), "c".to_string()])
        );
    }

    /// 摘要与指纹原语金值：锁定长度前缀摘要算法的绝对输出。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 摘要算法或长度前缀方案变化导致输出漂移时测试失败。
    #[test]
    fn digest_primitives_match_golden_values() {
        assert_eq!(
            digest_parts(["ab".to_string(), "c".to_string()]),
            "601d5476e2ccfe2c87a2bba7a322659734a05749d5b5aa781f513e4912db0d5f"
        );
        assert_eq!(
            payload_fingerprint(
                "purchase_order.void",
                "po-1",
                &TestFingerprintPayload {
                    idempotency_key: "raw-secret-key".to_string(),
                    reason: " 重复采购 ".to_string(),
                },
            )
            .unwrap(),
            "8c575956d6103de95a3f096b0ad4305bf7c3e49347ad1bc8907796696ed7d58b"
        );
    }

    /// 收据身份金值：锁定规范摘要与整串摘要历史形态的绝对输出。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 身份摘要或历史整串形态变化导致输出漂移时测试失败。
    #[test]
    fn receipt_identity_matches_golden_values() {
        let identity = PurchaseCommandReceipt::<TestReceipt>::identity(
            "purchase-order-command-",
            "actor-1",
            "purchase_order.update",
            Some("po-1"),
            "key-1",
            LegacyReceiptIdScheme::None,
        )
        .unwrap();
        assert_eq!(
            identity.receipt_id(),
            "purchase-order-command-f8724084cfe6b2b4af4a30ac33fc56779c6c391b7ea90c7fc7c10768dad5c5a5"
        );
        let legacy = PurchaseCommandReceipt::<TestReceipt>::identity(
            "purchase-submit-command-",
            "actor-1",
            "purchase_order.submit",
            Some("po-1"),
            "legacy-key",
            LegacyReceiptIdScheme::WholeStringJoined,
        )
        .unwrap();
        assert_eq!(
            legacy.id_candidates()[1],
            "purchase-submit-command-b2c14973cf49ab8f62e36d5f3939588469088bec3616c6af63425940ef5df273"
        );
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
    fn identity_is_stable_partitioned_and_key_safe() {
        let key = "raw-secret-idempotency-key";
        let identity = PurchaseCommandReceipt::<TestReceipt>::identity(
            "purchase-order-command-",
            "actor-1",
            "purchase_order.update",
            Some("po-1"),
            key,
            LegacyReceiptIdScheme::None,
        )
        .unwrap();

        assert_eq!(
            identity.receipt_id(),
            PurchaseCommandReceipt::<TestReceipt>::identity(
                "purchase-order-command-",
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                key,
                LegacyReceiptIdScheme::None,
            )
            .unwrap()
            .receipt_id()
        );
        assert!(!identity.receipt_id().contains(key));
        assert_ne!(
            identity.receipt_id(),
            PurchaseCommandReceipt::<TestReceipt>::identity(
                "purchase-order-command-",
                "actor-2",
                "purchase_order.update",
                Some("po-1"),
                key,
                LegacyReceiptIdScheme::None,
            )
            .unwrap()
            .receipt_id()
        );
        assert_ne!(
            identity.receipt_id(),
            PurchaseCommandReceipt::<TestReceipt>::identity(
                "purchase-order-command-",
                "actor-1",
                "purchase_order.void",
                Some("po-1"),
                key,
                LegacyReceiptIdScheme::None,
            )
            .unwrap()
            .receipt_id()
        );
        assert_ne!(
            identity.receipt_id(),
            PurchaseCommandReceipt::<TestReceipt>::identity(
                "purchase-order-command-",
                "actor-1",
                "purchase_order.update",
                Some("po-2"),
                key,
                LegacyReceiptIdScheme::None,
            )
            .unwrap()
            .receipt_id()
        );
        assert_ne!(
            identity.receipt_id(),
            PurchaseCommandReceipt::<TestReceipt>::identity(
                "purchase-order-command-",
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                "another-key",
                LegacyReceiptIdScheme::None,
            )
            .unwrap()
            .receipt_id()
        );
        assert_eq!(identity.id_candidates(), vec![identity.receipt_id()]);
    }

    /// 验证依据创建路径（目标执行后才存在）沿用历史三身份摘要形态。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 新 ID 与历史 `digest_parts([actor, action, key])` 形态不一致时测试失败。
    #[test]
    fn targetless_identity_matches_legacy_creation_shape() {
        let identity = PurchaseCommandReceipt::<TestReceipt>::identity(
            "purchase-order-create-command-",
            "actor-1",
            "purchase_order.create_from_basis",
            None,
            "key-1",
            LegacyReceiptIdScheme::None,
        )
        .unwrap();
        let expected = format!(
            "purchase-order-create-command-{}",
            digest_parts([
                "actor-1".to_string(),
                "purchase_order.create_from_basis".to_string(),
                "key-1".to_string(),
            ])
        );
        assert_eq!(identity.receipt_id(), expected);
    }

    /// 验证整串摘要历史形态保留为查询候选且新写入使用规范摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 历史 ID 不在候选或新写入 ID 非规范摘要时测试失败。
    #[test]
    fn whole_string_legacy_identity_remains_lookup_candidate() {
        let identity = PurchaseCommandReceipt::<TestReceipt>::identity(
            "purchase-submit-command-",
            "actor-1",
            "purchase_order.submit",
            Some("po-1"),
            "legacy-key",
            LegacyReceiptIdScheme::WholeStringJoined,
        )
        .unwrap();
        let legacy = format!(
            "purchase-submit-command-{}",
            hex::encode(Sha256::digest(b"actor-1|purchase_order.submit|po-1|legacy-key"))
        );
        assert!(identity.id_candidates().contains(&legacy.as_str()));
        assert_ne!(identity.receipt_id(), legacy);
        assert_eq!(
            identity.receipt_id(),
            format!(
                "purchase-submit-command-{}",
                digest_parts([
                    "actor-1".to_string(),
                    "purchase_order.submit".to_string(),
                    "po-1".to_string(),
                    "legacy-key".to_string(),
                ])
            )
        );
    }

    /// 验证空身份输入被拒绝。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 前缀、操作人、动作、目标或幂等键为空未被拒绝时测试失败。
    #[test]
    fn identity_rejects_empty_identity_fields() {
        assert!(PurchaseCommandReceipt::<TestReceipt>::identity(
            " ",
            "actor-1",
            "purchase_order.update",
            Some("po-1"),
            "key-1",
            LegacyReceiptIdScheme::None,
        )
        .is_err());
        assert!(PurchaseCommandReceipt::<TestReceipt>::identity(
            "prefix-",
            " ",
            "purchase_order.update",
            Some("po-1"),
            "key-1",
            LegacyReceiptIdScheme::None,
        )
        .is_err());
        assert!(PurchaseCommandReceipt::<TestReceipt>::identity(
            "prefix-",
            "actor-1",
            " ",
            Some("po-1"),
            "key-1",
            LegacyReceiptIdScheme::None,
        )
        .is_err());
        assert!(PurchaseCommandReceipt::<TestReceipt>::identity(
            "prefix-",
            "actor-1",
            "purchase_order.update",
            Some(" "),
            "key-1",
            LegacyReceiptIdScheme::None,
        )
        .is_err());
        assert!(PurchaseCommandReceipt::<TestReceipt>::identity(
            "prefix-",
            "actor-1",
            "purchase_order.update",
            Some("po-1"),
            " ",
            LegacyReceiptIdScheme::None,
        )
        .is_err());
    }

    /// 验证请求指纹稳定、随载荷变化且不泄露敏感载荷。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 同载荷指纹漂移、异载荷未冲突或摘要含原始键时测试失败。
    #[test]
    fn payload_fingerprint_is_stable_payload_sensitive_and_leak_free() {
        let payload = TestFingerprintPayload {
            idempotency_key: "raw-secret-key".to_string(),
            reason: " 重复采购 ".to_string(),
        };
        let fingerprint = payload_fingerprint("purchase_order.void", "po-1", &payload).unwrap();
        assert_eq!(
            fingerprint,
            payload_fingerprint("purchase_order.void", "po-1", &payload).unwrap()
        );
        assert_ne!(
            fingerprint,
            payload_fingerprint(
                "purchase_order.void",
                "po-1",
                &TestFingerprintPayload {
                    reason: "供应商错误".to_string(),
                    ..payload.clone()
                },
            )
            .unwrap()
        );
        assert_ne!(
            fingerprint,
            payload_fingerprint("purchase_order.void", "po-2", &payload).unwrap()
        );
        assert_eq!(fingerprint.len(), 64);
        assert!(fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(!fingerprint.contains("raw-secret-key"));
        let payload_json = serde_json::to_string(&payload).unwrap();
        assert_eq!(
            fingerprint,
            digest_parts([
                "purchase_order.void".to_string(),
                "po-1".to_string(),
                payload_json,
            ])
        );
    }

    /// 验证同指纹回放原结果、异指纹冲突且消息形态与存量一致。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 收据不能稳定回放、异载荷未冲突或消息形态漂移时测试失败。
    #[test]
    fn message_round_trips_same_payload_and_rejects_different_payload() {
        let fingerprint =
            payload_fingerprint("purchase_order.update", "po-1", &(1_u64, "payload-a")).unwrap();
        let receipt = PurchaseCommandReceipt::new(
            fingerprint.clone(),
            TestReceipt {
                purchase_order_id: "po-1".to_string(),
                lock_version: 2,
            },
        );
        let message = receipt.encode_message().unwrap();
        let legacy_wire = format!(
            "command_sha256={fingerprint};result={}",
            serde_json::to_string(&TestReceipt {
                purchase_order_id: "po-1".to_string(),
                lock_version: 2,
            })
            .unwrap()
        );
        assert_eq!(message, legacy_wire);

        let replayed = PurchaseCommandReceipt::<TestReceipt>::decode(
            &audit_fixture(message.clone()),
            "actor-1",
            "purchase_order.update",
            Some("po-1"),
            &fingerprint,
        )
        .unwrap();
        assert_eq!(replayed.fingerprint(), fingerprint);
        assert_eq!(replayed.into_payload().lock_version, 2);

        let different = payload_fingerprint("purchase_order.update", "po-1", &(1_u64, "payload-b")).unwrap();
        assert_eq!(
            PurchaseCommandReceipt::<TestReceipt>::decode(
                &audit_fixture(message),
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                &different,
            ),
            Err(PurchaseCommandReceiptError::PayloadConflict)
        );
    }

    /// 验证身份任一维度不一致时返回身份冲突。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 操作人、动作、资源类型、目标或成功标记不一致未被识别时测试失败。
    #[test]
    fn decode_rejects_wrong_identity() {
        let fingerprint =
            payload_fingerprint("purchase_order.update", "po-1", &(1_u64, "payload-a")).unwrap();
        let message = PurchaseCommandReceipt::new(
            fingerprint.clone(),
            TestReceipt {
                purchase_order_id: "po-1".to_string(),
                lock_version: 2,
            },
        )
        .encode_message()
        .unwrap();

        let wrong_actor = AuditLog::new(
            "receipt-1".to_string(),
            AuditLogData {
                actor_id: "actor-2".to_string(),
                ..audit_data(Some(message.clone()))
            },
        )
        .unwrap();
        assert_eq!(
            PurchaseCommandReceipt::<TestReceipt>::decode(
                &wrong_actor,
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                &fingerprint,
            ),
            Err(PurchaseCommandReceiptError::IdentityMismatch)
        );
        assert_eq!(
            PurchaseCommandReceipt::<TestReceipt>::decode(
                &audit_fixture(message.clone()),
                "actor-1",
                "purchase_order.void",
                Some("po-1"),
                &fingerprint,
            ),
            Err(PurchaseCommandReceiptError::IdentityMismatch)
        );
        assert_eq!(
            PurchaseCommandReceipt::<TestReceipt>::decode(
                &audit_fixture(message.clone()),
                "actor-1",
                "purchase_order.update",
                Some("po-2"),
                &fingerprint,
            ),
            Err(PurchaseCommandReceiptError::IdentityMismatch)
        );

        let failed = AuditLog::new(
            "receipt-1".to_string(),
            AuditLogData {
                success: false,
                ..audit_data(Some(message))
            },
        )
        .unwrap();
        assert_eq!(
            PurchaseCommandReceipt::<TestReceipt>::decode(
                &failed,
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                &fingerprint,
            ),
            Err(PurchaseCommandReceiptError::IdentityMismatch)
        );
    }

    /// 验证坏消息形态与坏结果 JSON 返回稳定内部错误。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 缺失消息、非法前缀、缺少结果段或结果 JSON 损坏未被识别时测试失败。
    #[test]
    fn decode_rejects_bad_format() {
        let fingerprint =
            payload_fingerprint("purchase_order.update", "po-1", &(1_u64, "payload-a")).unwrap();
        let missing_message = AuditLog::new("receipt-1".to_string(), audit_data(None)).unwrap();
        assert!(matches!(
            PurchaseCommandReceipt::<TestReceipt>::decode(
                &missing_message,
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                &fingerprint,
            ),
            Err(PurchaseCommandReceiptError::Corrupted(_))
        ));

        let bad_prefix = audit_fixture("garbage".to_string());
        assert!(matches!(
            PurchaseCommandReceipt::<TestReceipt>::decode(
                &bad_prefix,
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                &fingerprint,
            ),
            Err(PurchaseCommandReceiptError::Corrupted(_))
        ));

        let missing_result = audit_fixture("command_sha256=abc".to_string());
        assert!(matches!(
            PurchaseCommandReceipt::<TestReceipt>::decode(
                &missing_result,
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                &fingerprint,
            ),
            Err(PurchaseCommandReceiptError::Corrupted(_))
        ));

        let bad_result = audit_fixture(format!("command_sha256={fingerprint};result=not-json"));
        assert_eq!(
            PurchaseCommandReceipt::<TestReceipt>::decode(
                &bad_result,
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                &fingerprint,
            ),
            Err(PurchaseCommandReceiptError::Corrupted(
                "采购命令幂等收据结果非法".to_string()
            ))
        );
    }

    /// 验证自定义 wire 编解码（存量管道分隔形态）可回放且坏形态失败。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 管道分隔收据不能回放或字段缺失未失败时测试失败。
    #[test]
    fn custom_wire_codec_round_trips_legacy_pipe_format() {
        let fingerprint = "b".repeat(64);
        let message = PurchaseCommandReceipt::new(
            fingerprint.clone(),
            PipeReceipt {
                purchase_no: "PO-1".to_string(),
                lock_version: 2,
            },
        )
        .encode_message()
        .unwrap();
        assert!(message.ends_with(";result=PO-1|2"));
        let replayed = PurchaseCommandReceipt::<PipeReceipt>::decode(
            &audit_fixture(message),
            "actor-1",
            "purchase_order.update",
            Some("po-1"),
            &fingerprint,
        )
        .unwrap();
        assert_eq!(
            replayed.into_payload(),
            PipeReceipt {
                purchase_no: "PO-1".to_string(),
                lock_version: 2,
            }
        );

        let truncated = audit_fixture(format!("command_sha256={fingerprint};result=PO-1"));
        assert!(matches!(
            PurchaseCommandReceipt::<PipeReceipt>::decode(
                &truncated,
                "actor-1",
                "purchase_order.update",
                Some("po-1"),
                &fingerprint,
            ),
            Err(PurchaseCommandReceiptError::Corrupted(_))
        ));
    }
}
