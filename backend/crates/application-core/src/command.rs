//! 业务命令共享的版本化稳定指纹基元。

use std::fmt::Display;

use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const V1_PREFIX: &str = "sha256-v1:";

/// 版本化、长度前缀编码的 SHA-256 命令指纹。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CommandFingerprint(String);

impl CommandFingerprint {
    /// 按输入顺序对全部分量进行长度前缀编码并形成 v1 指纹。
    ///
    /// 本方法不使用 `Debug`、JSON Map 顺序或分隔符拼接；调用方必须显式固定
    /// 字段顺序，集合字段必须先按其业务语义规范排序。
    ///
    /// # 参数
    /// * `parts` - 按固定顺序的指纹分量
    ///
    /// # 返回
    /// 返回版本化持久指纹。
    ///
    /// # 错误
    /// 无。
    pub fn from_parts(parts: impl IntoIterator<Item = String>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"command-fingerprint-v1");
        feed_length_prefixed(&mut hasher, parts);
        Self(format!("{V1_PREFIX}{}", hex::encode(hasher.finalize())))
    }

    /// 解析已持久化的 v1 指纹。
    ///
    /// 摘要统一转小写归一化，大小写输入视为同一指纹。
    ///
    /// # 参数
    /// * `value` - 持久化指纹字符串
    ///
    /// # 返回
    /// 返回归一化后的指纹。
    ///
    /// # 错误
    /// 前缀缺失或摘要非 64 位十六进制时返回错误。
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let digest = value
            .strip_prefix(V1_PREFIX)
            .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| Error::from("命令指纹格式无效"))?;
        Ok(Self(format!("{V1_PREFIX}{}", digest.to_ascii_lowercase())))
    }

    /// 返回稳定持久化字符串。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回含版本前缀的持久化字符串。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 返回 64 位摘要部分。
    ///
    /// 构造器保证前缀存在并做小写归一化，正常路径恒成立。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 64 位小写十六进制摘要。
    ///
    /// # 错误
    /// 无；非法形态在 debug 断言，release 兜底为空。
    pub fn digest_hex(&self) -> &str {
        debug_assert!(self.0.starts_with(V1_PREFIX), "已验证指纹必须有 v1 前缀");
        self.0.strip_prefix(V1_PREFIX).unwrap_or("")
    }
}

/// 以长度前缀 feeding 给定哈希器，版本化与历史路径共用内核。
fn feed_length_prefixed(hasher: &mut Sha256, parts: impl IntoIterator<Item = impl AsRef<[u8]>>) {
    for part in parts {
        let bytes = part.as_ref();
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
}

impl<'de> Deserialize<'de> for CommandFingerprint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// 不保存原始幂等键的稳定命令身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandIdentity {
    current_id: String,
    legacy_ids: Vec<String>,
}

impl CommandIdentity {
    /// 形成 v1 身份，并登记只读兼容查询使用的历史 ID。
    ///
    /// # 参数
    /// * `prefix` - 收据 ID 前缀
    /// * `parts` - 当前身份分量
    /// * `legacy_ids` - 历史兼容候选 ID
    ///
    /// # 返回
    /// 返回当前与历史候选身份。
    ///
    /// # 错误
    /// 前缀为空时返回错误。
    pub fn new(
        prefix: &str,
        parts: impl IntoIterator<Item = String>,
        legacy_ids: impl IntoIterator<Item = String>,
    ) -> Result<Self> {
        if prefix.trim().is_empty() {
            return Err(Error::from("命令身份前缀不能为空"));
        }
        let fingerprint = CommandFingerprint::from_parts(parts);
        let current_id = format!("{prefix}{}", fingerprint.digest_hex());
        let mut legacy_ids = legacy_ids.into_iter().filter(|id| id != &current_id).collect::<Vec<_>>();
        legacy_ids.sort();
        legacy_ids.dedup();
        Ok(Self { current_id, legacy_ids })
    }

    /// 返回新写入使用的 v1 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回当前身份 ID。
    ///
    /// # 错误
    /// 无。
    pub fn current_id(&self) -> &str {
        &self.current_id
    }

    /// 判断给定 ID 是否为当前或历史候选，避免纯检查场景分配。
    ///
    /// # 参数
    /// * `id` - 待检查的候选 ID
    ///
    /// # 返回
    /// 命中时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn contains_candidate(&self, id: &str) -> bool {
        self.current_id == id || self.legacy_ids.iter().any(|legacy| legacy == id)
    }

    /// 返回按当前优先、历史其次排列的候选迭代器，避免纯检查场景分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回当前与历史候选的 borrowed 迭代器。
    ///
    /// # 错误
    /// 无。
    pub fn candidates_iter(&self) -> impl Iterator<Item = &str> + '_ {
        std::iter::once(self.current_id.as_str()).chain(self.legacy_ids.iter().map(String::as_str))
    }

    /// 返回按当前优先、历史其次排列的查询候选 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 owned 候选集合；纯检查改用 `contains_candidate`。
    ///
    /// # 错误
    /// 无。
    pub fn candidates(&self) -> Vec<String> {
        self.candidates_iter().map(str::to_string).collect()
    }
}

/// Repository 返回的命令收据最小事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandReceiptFact {
    pub id: String,
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub success: bool,
    pub message: Option<String>,
}

impl CommandReceiptFact {
    /// 由必填收据身份字段构造最小事实。
    ///
    /// # 参数
    /// * `id` - 收据主键
    /// * `actor_id` - 操作人身份
    /// * `action` - 动作名称
    /// * `resource_type` - 资源类型
    ///
    /// # 返回
    /// 返回资源、结果与消息为空的收据事实。
    ///
    /// # 错误
    /// 无。
    pub fn new(
        id: impl Into<String>,
        actor_id: impl Into<String>,
        action: impl Into<String>,
        resource_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            actor_id: actor_id.into(),
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: None,
            success: false,
            message: None,
        }
    }

    /// 设置目标资源 ID。
    ///
    /// # 参数
    /// * `resource_id` - 目标资源 ID
    ///
    /// # 返回
    /// 返回更新后的收据事实。
    ///
    /// # 错误
    /// 无。
    pub fn with_resource_id(self, resource_id: impl Into<String>) -> Self {
        self.with_resource_id_opt(Some(resource_id.into()))
    }

    /// 设置目标资源 ID（`None` 保持缺省）。
    ///
    /// # 参数
    /// * `resource_id` - 目标资源 ID
    ///
    /// # 返回
    /// 返回更新后的收据事实。
    ///
    /// # 错误
    /// 无。
    pub fn with_resource_id_opt(mut self, resource_id: Option<String>) -> Self {
        self.resource_id = resource_id;
        self
    }

    /// 设置执行结果。
    ///
    /// # 参数
    /// * `success` - 是否执行成功
    ///
    /// # 返回
    /// 返回更新后的收据事实。
    ///
    /// # 错误
    /// 无。
    pub fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    /// 设置收据消息。
    ///
    /// # 参数
    /// * `message` - 收据消息
    ///
    /// # 返回
    /// 返回更新后的收据事实。
    ///
    /// # 错误
    /// 无。
    pub fn with_message(self, message: impl Into<String>) -> Self {
        self.with_message_opt(Some(message.into()))
    }

    /// 设置收据消息（`None` 保持缺省）。
    ///
    /// # 参数
    /// * `message` - 收据消息
    ///
    /// # 返回
    /// 返回更新后的收据事实。
    ///
    /// # 错误
    /// 无。
    pub fn with_message_opt(mut self, message: Option<String>) -> Self {
        self.message = message;
        self
    }
}

/// 已提交收据与当前请求的纯匹配结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandReceiptMatch {
    /// 同一命令载荷，可回放首次资源 ID。
    SamePayload(String),
    /// 同一命令身份已被不同载荷占用。
    DifferentPayload,
    /// 持久化行身份或收据形态损坏。
    Corrupted,
}

/// 共享、版本化的业务命令收据值对象。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandReceipt {
    identity: CommandIdentity,
    actor_id: String,
    action: String,
    resource_type: String,
    fingerprint: CommandFingerprint,
    legacy_fingerprints: Vec<String>,
}

/// 去首尾空白后非空校验，幂等键与资源 ID 共用空白判定。
fn require_non_blank<'a>(value: &'a str, message: &str) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::from(message));
    }
    Ok(trimmed)
}

/// 规范化幂等键，两构造入口共用。
fn normalize_idempotency_key(idempotency_key: &str) -> Result<&str> {
    require_non_blank(idempotency_key, "操作号不能为空")
}

/// 载荷序列化失败统一映射，两条序列化路径共用。
fn payload_serialize_error(error: impl Display) -> Error {
    Error::from(format!("业务命令请求序列化失败: {error}"))
}

/// 收据身份三元组，组装入口共用，避免长参数列。
struct ReceiptHeader<'a> {
    actor_id: &'a str,
    action: &'a str,
    resource_type: &'a str,
}

/// 组装收据主体，两构造入口共用身份去重与字段填充。
fn assemble(
    prefix: &str,
    identity_parts: impl IntoIterator<Item = String>,
    legacy_digest: &str,
    header: ReceiptHeader<'_>,
    fingerprint: CommandFingerprint,
    legacy_fingerprint: String,
) -> Result<CommandReceipt> {
    let legacy_identity = format!("{prefix}{legacy_digest}");
    let identity = CommandIdentity::new(prefix, identity_parts, [legacy_identity])?;
    Ok(CommandReceipt {
        identity,
        actor_id: header.actor_id.to_string(),
        action: header.action.to_string(),
        resource_type: header.resource_type.to_string(),
        fingerprint,
        legacy_fingerprints: vec![legacy_fingerprint],
    })
}

impl CommandReceipt {
    /// 从可序列化请求形成规范 JSON v1 收据，并保留旧 JSON/摘要兼容候选。
    ///
    /// # 参数
    /// * `prefix` - 收据 ID 前缀
    /// * `actor_id` - 操作人身份
    /// * `action` - 动作名称
    /// * `resource_type` - 资源类型
    /// * `idempotency_key` - 调用方幂等键（首尾空白忽略，不持久化原文）
    /// * `payload` - 待规范化的请求载荷
    ///
    /// # 返回
    /// 返回版本化收据。
    ///
    /// # 错误
    /// 幂等键为空或载荷序列化失败时返回错误。
    pub fn from_payload<T: Serialize>(
        prefix: &str,
        actor_id: &str,
        action: &str,
        resource_type: &str,
        idempotency_key: &str,
        payload: &T,
    ) -> Result<Self> {
        let key = normalize_idempotency_key(idempotency_key)?;
        let canonical_payload = canonical_json(payload)?;
        let legacy_payload = serde_json::to_string(payload).map_err(payload_serialize_error)?;
        let fingerprint = CommandFingerprint::from_parts([
            action.to_string(),
            resource_type.to_string(),
            canonical_payload,
        ]);
        assemble(
            prefix,
            [actor_id, action, resource_type, key].into_iter().map(str::to_string),
            &legacy_compat::digest_parts(&[actor_id, action, resource_type, key]),
            ReceiptHeader { actor_id, action, resource_type },
            fingerprint,
            legacy_compat::digest_parts(&[action, resource_type, &legacy_payload]),
        )
    }

    /// 从资源定位命令的固定顺序字段形成 v1 收据。
    ///
    /// 兼容候选使用历史 `actor|action|resource_id|key` 身份和无版本
    /// 长度前缀指纹；新写入不保存原始幂等键。
    ///
    /// # 参数
    /// * `prefix` - 收据 ID 前缀
    /// * `actor_id` - 操作人身份
    /// * `action` - 动作名称
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 目标资源 ID
    /// * `idempotency_key` - 调用方幂等键（首尾空白忽略，不持久化原文）
    /// * `fingerprint_parts` - 指纹分量
    ///
    /// # 返回
    /// 返回版本化收据。
    ///
    /// # 错误
    /// 幂等键或资源 ID 为空时返回错误。
    pub fn from_resource_parts(
        prefix: &str,
        actor_id: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        idempotency_key: &str,
        fingerprint_parts: impl IntoIterator<Item = String>,
    ) -> Result<Self> {
        let key = normalize_idempotency_key(idempotency_key)?;
        require_non_blank(resource_id, "命令资源 ID 不能为空")?;
        let fingerprint_parts = fingerprint_parts.into_iter().collect::<Vec<_>>();
        let legacy_fingerprint_parts = fingerprint_parts.iter().map(String::as_str).collect::<Vec<_>>();
        let legacy_fingerprint = legacy_compat::digest_parts(&legacy_fingerprint_parts);
        assemble(
            prefix,
            [actor_id, action, resource_type, resource_id, key].into_iter().map(str::to_string),
            &legacy_compat::pipe_identity(actor_id, action, resource_id, key),
            ReceiptHeader { actor_id, action, resource_type },
            CommandFingerprint::from_parts(fingerprint_parts),
            legacy_fingerprint,
        )
    }

    /// 返回新写入使用的收据 ID。
    pub fn id(&self) -> &str {
        self.identity.current_id()
    }

    /// 返回当前及历史收据查询候选 ID。
    pub fn id_candidates(&self) -> Vec<String> {
        self.identity.candidates()
    }

    /// 返回收据所属操作人。
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    /// 返回收据动作。
    pub fn action(&self) -> &str {
        &self.action
    }

    /// 返回收据结果资源类型。
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// 返回 v1 持久化消息；可追加权限安全的说明文本。
    ///
    /// # 参数
    /// * `detail` - 追加的说明文本
    ///
    /// # 返回
    /// 返回指纹承载段与说明的展示拼接。
    ///
    /// # 错误
    /// 无。
    pub fn message(&self, detail: Option<&str>) -> String {
        let base = format!("command_fingerprint={}", self.fingerprint.as_str());
        match detail {
            Some(detail) => format!("{base}; {detail}"),
            None => base,
        }
    }

    /// 返回结构化指纹承载段，供展示拼接外的调用方直接使用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `(当前指纹, 历史指纹)`。
    ///
    /// # 错误
    /// 无。
    pub fn fingerprints(&self) -> (&CommandFingerprint, &[String]) {
        (&self.fingerprint, &self.legacy_fingerprints)
    }

    /// 校验最小持久化事实并分类回放结果。
    ///
    /// 身份不一致（含候选缺失与字段不一致）与未成功回放视为不可回放；
    /// 形态损坏（缺消息、缺资源）视为损坏；指纹不一致视为载荷不同。
    ///
    /// # 参数
    /// * `fact` - 最小持久化事实
    ///
    /// # 返回
    /// 返回回放分类。
    ///
    /// # 错误
    /// 无。
    pub fn match_fact(&self, fact: &CommandReceiptFact) -> CommandReceiptMatch {
        // 注意：当前仍返回 Corrupted 以保持调用方匹配语义；
        // 未成功回放与身份不符的独立分类待调用方同步后收敛。
        if let Err(matched) = self.check_identity(fact) {
            return matched;
        }
        let fingerprint = match extract_persisted_fingerprint(fact.message.as_deref()) {
            Ok(fingerprint) => fingerprint,
            Err(matched) => return matched,
        };
        if !self.matches_fingerprint(fingerprint) {
            return CommandReceiptMatch::DifferentPayload;
        }
        fact.resource_id
            .clone()
            .map(CommandReceiptMatch::SamePayload)
            .unwrap_or(CommandReceiptMatch::Corrupted)
    }

    /// 校验候选身份、成功标志与身份字段三元组。
    fn check_identity(&self, fact: &CommandReceiptFact) -> std::result::Result<(), CommandReceiptMatch> {
        if !self.identity.contains_candidate(&fact.id)
            || fact.actor_id != self.actor_id
            || fact.action != self.action
            || fact.resource_type != self.resource_type
        {
            return Err(CommandReceiptMatch::Corrupted);
        }
        if !fact.success {
            return Err(CommandReceiptMatch::Corrupted);
        }
        Ok(())
    }

    /// 比对当前与历史指纹。
    fn matches_fingerprint(&self, persisted: PersistedFingerprint<'_>) -> bool {
        match persisted {
            PersistedFingerprint::Current(value) => value == self.fingerprint.as_str(),
            PersistedFingerprint::Legacy(value) => {
                self.legacy_fingerprints.iter().any(|legacy| legacy == value)
            },
        }
    }
}

/// 持久化消息中的指纹承载段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistedFingerprint<'a> {
    Current(&'a str),
    Legacy(&'a str),
}

/// 从自由文本消息提取结构化指纹段。
fn extract_persisted_fingerprint(
    message: Option<&str>,
) -> std::result::Result<PersistedFingerprint<'_>, CommandReceiptMatch> {
    let Some(message) = message else {
        return Err(CommandReceiptMatch::Corrupted);
    };
    let persisted = message.split_once(';').map(|(head, _)| head).unwrap_or(message);
    if let Some(value) = persisted.strip_prefix("command_fingerprint=") {
        return Ok(PersistedFingerprint::Current(value));
    }
    if let Some(value) = persisted.strip_prefix("command_sha256=") {
        return Ok(PersistedFingerprint::Legacy(value));
    }
    Err(CommandReceiptMatch::Corrupted)
}

fn canonical_json<T: Serialize>(payload: &T) -> Result<String> {
    let value = serde_json::to_value(payload).map_err(payload_serialize_error)?;
    let mut output = String::new();
    write_canonical_json(&value, &mut output)?;
    Ok(output)
}

/// 叶子 JSON 标量序列化失败路径共用收据序列化错误。
fn write_scalar_json(value: &impl Serialize, output: &mut String) -> Result<()> {
    let raw = serde_json::to_string(value).map_err(payload_serialize_error)?;
    output.push_str(&raw);
    Ok(())
}

fn write_canonical_json(value: &serde_json::Value, output: &mut String) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            output.push('{');
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            for (index, key) in keys.into_iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_scalar_json(key, output)?;
                output.push(':');
                write_canonical_json(&map[key], output)?;
            }
            output.push('}');
        },
        serde_json::Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical_json(value, output)?;
            }
            output.push(']');
        },
        value => write_scalar_json(value, output)?,
    }
    Ok(())
}

/// 历史无版本指纹内核（独立版本适配，核心 v1 路径不依赖它）。
mod legacy_compat {
    use sha2::{Digest, Sha256};

    use super::feed_length_prefixed;

    /// 计算历史无版本长度前缀摘要。
    pub(super) fn digest_parts(parts: &[&str]) -> String {
        let mut hasher = Sha256::new();
        feed_length_prefixed(&mut hasher, parts);
        hex::encode(hasher.finalize())
    }

    /// 历史 `actor|action|resource_id|key` 管道身份摘要。
    pub(super) fn pipe_identity(actor_id: &str, action: &str, resource_id: &str, key: &str) -> String {
        hex::encode(Sha256::digest(format!("{actor_id}|{action}|{resource_id}|{key}").as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use sha2::{Digest, Sha256};

    use super::legacy_compat::digest_parts as legacy_digest_parts;
    use super::{CommandFingerprint, CommandReceipt, CommandReceiptFact, CommandReceiptMatch};

    #[derive(Serialize)]
    struct Payload {
        amount: u32,
        idempotency_key: String,
    }

    #[test]
    fn length_prefix_prevents_concatenation_collision() {
        assert_ne!(
            CommandFingerprint::from_parts(["ab".to_string(), "c".to_string()]),
            CommandFingerprint::from_parts(["a".to_string(), "bc".to_string()])
        );
    }

    #[test]
    fn wire_format_is_versioned_and_round_trips() {
        let fingerprint = CommandFingerprint::from_parts(["payload".to_string()]);
        assert!(fingerprint.as_str().starts_with("sha256-v1:"));
        assert_eq!(CommandFingerprint::parse(fingerprint.as_str()).unwrap(), fingerprint);
    }

    #[test]
    fn receipt_hides_raw_key_and_rejects_different_payload() {
        let first = Payload { amount: 100, idempotency_key: "secret-operation-key".to_string() };
        let changed = Payload { amount: 200, idempotency_key: first.idempotency_key.clone() };
        let receipt = CommandReceipt::from_payload(
            "receipt-",
            "actor-1",
            "payment.commit",
            "payment",
            &first.idempotency_key,
            &first,
        )
        .unwrap();
        assert!(!receipt.id().contains(&first.idempotency_key));
        assert!(!receipt.message(None).contains(&first.idempotency_key));
        let fact = CommandReceiptFact::new(receipt.id().to_string(), "actor-1", "payment.commit", "payment")
            .with_resource_id("payment-1")
            .with_success(true)
            .with_message(receipt.message(None));
        assert_eq!(receipt.match_fact(&fact), CommandReceiptMatch::SamePayload("payment-1".to_string()));
        let changed_receipt = CommandReceipt::from_payload(
            "receipt-",
            "actor-1",
            "payment.commit",
            "payment",
            &changed.idempotency_key,
            &changed,
        )
        .unwrap();
        assert_eq!(changed_receipt.match_fact(&fact), CommandReceiptMatch::DifferentPayload);
    }

    #[test]
    fn canonical_json_ignores_object_key_insertion_order() {
        let left = serde_json::json!({"z": 1, "nested": {"b": 2, "a": 1}});
        let right = serde_json::json!({"nested": {"a": 1, "b": 2}, "z": 1});
        let left =
            CommandReceipt::from_payload("receipt-", "actor-1", "object.commit", "object", "key-1", &left)
                .unwrap();
        let right =
            CommandReceipt::from_payload("receipt-", "actor-1", "object.commit", "object", "key-1", &right)
                .unwrap();
        assert_eq!(left.id(), right.id());
        assert_eq!(left.message(None), right.message(None));
    }

    #[test]
    fn historical_audit_receipt_remains_replayable() {
        let payload = Payload { amount: 100, idempotency_key: "legacy-key".to_string() };
        let receipt = CommandReceipt::from_payload(
            "receipt-",
            "actor-1",
            "payment.commit",
            "payment",
            &payload.idempotency_key,
            &payload,
        )
        .unwrap();
        let legacy_payload = serde_json::to_string(&payload).unwrap();
        let legacy_id = format!(
            "receipt-{}",
            legacy_digest_parts(&["actor-1", "payment.commit", "payment", "legacy-key"])
        );
        let legacy_fingerprint = legacy_digest_parts(&["payment.commit", "payment", &legacy_payload]);
        let fact = CommandReceiptFact::new(legacy_id.clone(), "actor-1", "payment.commit", "payment")
            .with_resource_id("payment-1")
            .with_success(true)
            .with_message(format!("command_sha256={legacy_fingerprint}"));
        assert!(receipt.id_candidates().contains(&legacy_id));
        assert_eq!(receipt.match_fact(&fact), CommandReceiptMatch::SamePayload("payment-1".to_string()));
    }

    #[test]
    fn receipt_fact_constructor_sets_identity_only() {
        let fact = CommandReceiptFact::new("receipt-1", "actor-1", "payment.commit", "payment");
        assert_eq!(fact.id, "receipt-1");
        assert_eq!(fact.actor_id, "actor-1");
        assert!(fact.resource_id.is_none());
        assert!(!fact.success);
        assert!(fact.message.is_none());
        let complete = CommandReceiptFact::new("receipt-1", "actor-1", "payment.commit", "payment")
            .with_resource_id("payment-1")
            .with_success(true)
            .with_message("command_fingerprint=sha256-v1:abc");
        assert_eq!(complete.resource_id.as_deref(), Some("payment-1"));
        assert!(complete.success);
        assert!(complete.message.is_some());
    }

    #[test]
    fn historical_work_item_receipt_remains_replayable_without_join_collision() {
        let receipt = CommandReceipt::from_resource_parts(
            "work-item-command-",
            "actor-1",
            "work_item.reassign",
            "work_item",
            "wi-1",
            "legacy-key",
            ["3".to_string(), "user-2".to_string(), "reason".to_string()],
        )
        .unwrap();
        let legacy_id = format!(
            "work-item-command-{}",
            hex::encode(Sha256::digest(b"actor-1|work_item.reassign|wi-1|legacy-key"))
        );
        let legacy_fingerprint = legacy_digest_parts(&["3", "user-2", "reason"]);
        let fact = CommandReceiptFact::new(legacy_id.clone(), "actor-1", "work_item.reassign", "work_item")
            .with_resource_id("wi-1")
            .with_success(true)
            .with_message(format!("command_sha256={legacy_fingerprint}; reason=safe"));
        assert!(receipt.id_candidates().contains(&legacy_id));
        assert_eq!(receipt.match_fact(&fact), CommandReceiptMatch::SamePayload("wi-1".to_string()));
        assert_ne!(
            CommandFingerprint::from_parts(["ab".to_string(), "c".to_string()]),
            CommandFingerprint::from_parts(["a".to_string(), "bc".to_string()])
        );
    }
}
