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
    /// 返回 64 位小写十六进制摘要；release 下前缀缺失时返回空字符串。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # Panics
    /// debug 构建下前缀缺失会触发断言。构造器与 `parse` 保证已验证指纹带 v1 前缀。
    pub fn digest_hex(&self) -> &str {
        debug_assert!(self.0.starts_with(V1_PREFIX), "已验证指纹必须有 v1 前缀");
        self.0.strip_prefix(V1_PREFIX).unwrap_or("")
    }
}

/// 以长度前缀 feeding 给定哈希器，用于版本化稳定摘要。
fn feed_length_prefixed(hasher: &mut Sha256, parts: impl IntoIterator<Item = impl AsRef<[u8]>>) {
    for part in parts {
        let bytes = part.as_ref();
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
}

impl<'de> Deserialize<'de> for CommandFingerprint {
    /// 将持久化字符串反序列化为 v1 指纹。
    ///
    /// 摘要按 `parse` 转成小写。
    ///
    /// # 参数
    /// * `deserializer` - 提供指纹字符串的反序列化器。
    ///
    /// # 返回
    /// 成功时返回归一化后的指纹。
    ///
    /// # 错误
    /// 输入不是字符串，或缺少 v1 前缀、摘要不是 64 位十六进制时，返回反序列化错误。
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// 不保存原始幂等键的稳定命令身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandIdentity {
    current_id: String,
}

impl CommandIdentity {
    /// 形成 v1 身份，仅登记当前命令 ID。
    ///
    /// # 参数
    /// * `prefix` - 收据 ID 前缀
    /// * `parts` - 当前身份分量
    ///
    /// # 返回
    /// 返回当前命令身份。
    ///
    /// # 错误
    /// `prefix` 去空白后为空时返回错误。
    pub fn new(prefix: &str, parts: impl IntoIterator<Item = String>) -> Result<Self> {
        if prefix.trim().is_empty() {
            return Err(Error::from("命令身份前缀不能为空"));
        }
        let fingerprint = CommandFingerprint::from_parts(parts);
        let current_id = format!("{prefix}{}", fingerprint.digest_hex());
        Ok(Self { current_id })
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
}

/// 共享、版本化的业务命令收据值对象。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandReceipt {
    identity: CommandIdentity,
    actor_id: String,
    action: String,
    resource_type: String,
    fingerprint: CommandFingerprint,
    scope_id: Option<String>,
    idempotency_key_hash: CommandFingerprint,
}

/// 去首尾空白后非空校验，幂等键与资源 ID 共用空白判定。
fn require_non_blank<'a>(value: &'a str, message: &str) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::from(message));
    }
    Ok(trimmed)
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
    scope_id: Option<&'a str>,
    idempotency_key: &'a str,
}

/// 组装收据主体，两构造入口共用身份去重与字段填充。
fn assemble(
    prefix: &str,
    identity_parts: impl IntoIterator<Item = String>,
    header: ReceiptHeader<'_>,
    fingerprint: CommandFingerprint,
) -> Result<CommandReceipt> {
    let identity = CommandIdentity::new(prefix, identity_parts)?;
    Ok(CommandReceipt {
        identity,
        actor_id: header.actor_id.to_string(),
        action: header.action.to_string(),
        resource_type: header.resource_type.to_string(),
        fingerprint,
        scope_id: header.scope_id.map(str::to_string),
        idempotency_key_hash: CommandFingerprint::from_parts([header.idempotency_key.to_string()]),
    })
}

impl CommandReceipt {
    /// 从可序列化请求形成规范 JSON v1 收据，仅保留当前规范化身份。
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
    /// `prefix` 或幂等键去空白后为空，或载荷序列化失败时返回错误。
    pub fn from_payload<T: Serialize>(
        prefix: &str,
        actor_id: &str,
        action: &str,
        resource_type: &str,
        idempotency_key: &str,
        payload: &T,
    ) -> Result<Self> {
        let key = require_non_blank(idempotency_key, "操作号不能为空")?;
        let canonical_payload = canonical_json(payload)?;
        let fingerprint = CommandFingerprint::from_parts([
            action.to_string(),
            resource_type.to_string(),
            canonical_payload,
        ]);
        assemble(
            prefix,
            [actor_id, action, resource_type, key].into_iter().map(str::to_string),
            ReceiptHeader { actor_id, action, resource_type, scope_id: None, idempotency_key: key },
            fingerprint,
        )
    }

    /// 从资源定位命令的固定顺序字段形成 v1 收据。
    ///
    /// 身份与载荷均使用版本化摘要，不保存原始幂等键。
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
    /// `prefix`、幂等键或资源 ID 去空白后为空时返回错误。
    pub fn from_resource_parts(
        prefix: &str,
        actor_id: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        idempotency_key: &str,
        fingerprint_parts: impl IntoIterator<Item = String>,
    ) -> Result<Self> {
        let key = require_non_blank(idempotency_key, "操作号不能为空")?;
        require_non_blank(resource_id, "命令资源 ID 不能为空")?;
        let fingerprint_parts = fingerprint_parts.into_iter().collect::<Vec<_>>();
        assemble(
            prefix,
            [actor_id, action, resource_type, resource_id, key].into_iter().map(str::to_string),
            ReceiptHeader {
                actor_id,
                action,
                resource_type,
                scope_id: Some(resource_id),
                idempotency_key: key,
            },
            CommandFingerprint::from_parts(fingerprint_parts),
        )
    }

    /// 返回新写入使用的收据 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回当前收据 ID。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn id(&self) -> &str {
        self.identity.current_id()
    }

    /// 返回收据所属操作人。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回操作人 ID。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    /// 返回收据动作。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回动作名称。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn action(&self) -> &str {
        &self.action
    }

    /// 返回收据结果资源类型。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回资源类型。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// 返回资源定位命令的目标资源 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `Some` 目标资源 ID。`from_payload` 形成的收据没有资源定位，返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn scope_id(&self) -> Option<&str> {
        self.scope_id.as_deref()
    }

    /// 返回规范化幂等键的版本化摘要，不暴露原始操作号。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回长度前缀 SHA-256 v1 摘要。
    /// # 错误
    /// 无。
    pub fn idempotency_key_hash(&self) -> &CommandFingerprint {
        &self.idempotency_key_hash
    }

    /// 返回当前规范化请求的版本化指纹。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回当前请求指纹。
    ///
    /// # 错误
    /// 无。
    pub fn fingerprint(&self) -> &CommandFingerprint {
        &self.fingerprint
    }
}

/// 把载荷收成键有序的规范 JSON，避免对象字段顺序影响指纹。
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

/// 对象键排序后写入规范 JSON，数组保持原顺序；标量序列化失败即失败。
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{StructuredCommandReceipt, StructuredReceiptMatch};

    #[test]
    fn fingerprint_has_unambiguous_lengths_and_versioned_round_trip() {
        let a = CommandFingerprint::from_parts(["a".to_string(), "bc".to_string()]);
        let b = CommandFingerprint::from_parts(["ab".to_string(), "c".to_string()]);
        assert_ne!(a, b);
        assert_eq!(CommandFingerprint::parse(a.as_str()).unwrap(), a);
        assert_eq!(
            serde_json::from_str::<CommandFingerprint>(&serde_json::to_string(&a).unwrap()).unwrap(),
            a
        );
        assert!(CommandFingerprint::parse("broken").is_err());
    }

    #[test]
    fn same_key_same_payload_replays_structured_fact_and_changed_payload_conflicts() {
        let make = |amount| {
            CommandReceipt::from_payload(
                "command-",
                "actor",
                "payment.commit",
                "payment",
                "raw-key",
                &json!({"amount":amount}),
            )
            .unwrap()
        };
        let original = make(10);
        let fact = StructuredCommandReceipt::from_command(&original).unwrap();
        assert_eq!(original.match_structured(&fact), StructuredReceiptMatch::SamePayload);
        assert_eq!(make(20).match_structured(&fact), StructuredReceiptMatch::DifferentPayload);
        assert!(!serde_json::to_string(&fact).unwrap().contains("raw-key"));
    }

    #[test]
    fn json_object_order_does_not_change_fingerprint() {
        let first =
            CommandReceipt::from_payload("c-", "actor", "update", "object", "key", &json!({"a":1,"b":2}))
                .unwrap();
        let second =
            CommandReceipt::from_payload("c-", "actor", "update", "object", "key", &json!({"b":2,"a":1}))
                .unwrap();
        assert_eq!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn target_actor_and_key_are_independent_identity_components() {
        let make = |actor: &str, target: &str, key: &str| {
            CommandReceipt::from_resource_parts(
                "c-",
                actor,
                "reassign",
                "task",
                target,
                key,
                ["payload".to_string()],
            )
            .unwrap()
        };
        let original = make("actor", "task", "key");
        for altered in
            [make("other", "task", "key"), make("actor", "other", "key"), make("actor", "task", "other")]
        {
            assert_ne!(original.id(), altered.id());
        }
        assert!(CommandReceipt::from_resource_parts("c-", "actor", "a", "r", " ", "key", []).is_err());
        assert!(CommandReceipt::from_payload("c-", "actor", "a", "r", " ", &1).is_err());
    }
}
