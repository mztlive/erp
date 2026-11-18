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
    /// 用按序写入的长度前缀与 SHA-256 生成可持久化的 v1 指纹，避免字段边界歧义。
    /// 调用方须固定字段顺序并按业务语义规范化集合；不要依赖 `Debug`、JSON 对象迭代顺序或分隔符拼接。
    pub fn from_parts(parts: impl IntoIterator<Item = String>) -> Self {
        Self::hash_parts(parts)
    }

    fn hash_parts(parts: impl IntoIterator<Item = impl AsRef<[u8]>>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"command-fingerprint-v1");
        for part in parts {
            let bytes = part.as_ref();
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        }

        Self(format!("{V1_PREFIX}{}", hex::encode(hasher.finalize())))
    }

    /// 回读持久化指纹时只接受 `sha256-v1:` 和 64 个十六进制字符，并归一化摘要大小写；版本或格式不符时不进入比对。
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let digest = value
            .strip_prefix(V1_PREFIX)
            .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| Error::from("命令指纹格式无效"))?;
        Ok(Self(format!("{V1_PREFIX}{}", digest.to_ascii_lowercase())))
    }

    /// 为持久化和跨层传递提供含算法版本的指纹文本，避免回读时丢失版本。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 取出 64 个小写十六进制字符，用于拼接带领域前缀的命令 ID。
    ///
    /// # Panics
    /// 构造和解析保证 v1 前缀；若内部不变式被破坏，调试构建触发断言，非调试构建返回空串。
    pub fn digest_hex(&self) -> &str {
        debug_assert!(self.0.starts_with(V1_PREFIX), "已验证指纹必须有 v1 前缀");
        self.0.strip_prefix(V1_PREFIX).unwrap_or("")
    }
}

impl<'de> Deserialize<'de> for CommandFingerprint {
    // 持久化回执回读只接受合法的 v1 指纹字符串，并将摘要转为小写后再参与幂等比对。
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
    /// 从领域前缀和有序身份分量生成 v1 命令 ID，供新写入和相同请求的只读查证共用。前缀裁剪后不得为空。
    pub fn new(prefix: &str, parts: impl IntoIterator<Item = String>) -> Result<Self> {
        Self::from_parts(prefix, parts)
    }

    fn from_parts(prefix: &str, parts: impl IntoIterator<Item = impl AsRef<[u8]>>) -> Result<Self> {
        if prefix.trim().is_empty() {
            return Err(Error::from("命令身份前缀不能为空"));
        }

        let fingerprint = CommandFingerprint::hash_parts(parts);
        let current_id = format!("{prefix}{}", fingerprint.digest_hex());
        Ok(Self { current_id })
    }

    /// 取新回执写入和按命令身份查证共用的 v1 ID。
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

// 计算回执前，幂等键和目标 ID 共用非空校验；返回裁剪后的文本，由各入口决定是否保留原文。
fn require_non_blank<'a>(value: &'a str, message: &str) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::from(message));
    }
    Ok(trimmed)
}

// 把构造 JSON 值与写出规范 JSON 的失败归为同一种命令载荷错误。
fn payload_serialize_error(error: impl Display) -> Error {
    Error::from(format!("业务命令请求序列化失败: {error}"))
}

struct ReceiptHeader<'a> {
    actor_id: &'a str,
    action: &'a str,
    resource_type: &'a str,
    scope_id: Option<&'a str>,
    idempotency_key: &'a str,
}

// 让两类回执共用身份分量顺序，把可选目标纳入命令 ID，只留下幂等键摘要而不保存原操作号。
fn assemble(
    prefix: &str,
    header: ReceiptHeader<'_>,
    fingerprint: CommandFingerprint,
) -> Result<CommandReceipt> {
    let identity_parts = [header.actor_id, header.action, header.resource_type]
        .into_iter()
        .chain(header.scope_id)
        .chain([header.idempotency_key]);
    let identity = CommandIdentity::from_parts(prefix, identity_parts)?;

    Ok(CommandReceipt {
        identity,
        actor_id: header.actor_id.to_string(),
        action: header.action.to_string(),
        resource_type: header.resource_type.to_string(),
        fingerprint,
        scope_id: header.scope_id.map(str::to_string),
        idempotency_key_hash: CommandFingerprint::hash_parts([header.idempotency_key]),
    })
}

impl CommandReceipt {
    /// 为不按目标资源定位的命令创建 v1 回执。操作人、动作、资源类型与去首尾空白的幂等键决定身份；
    /// 动作、资源类型与规范 JSON 决定载荷指纹，以区分原请求重试和换载荷重提。
    /// 前缀或幂等键为空白，以及载荷不能序列化时，均返回错误。
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
        let fingerprint = CommandFingerprint::hash_parts([action, resource_type, &canonical_payload]);

        assemble(
            prefix,
            ReceiptHeader { actor_id, action, resource_type, scope_id: None, idempotency_key: key },
            fingerprint,
        )
    }

    /// 为指向资源的命令创建 v1 回执，使相同操作号可按目标区分。调用方须按固定顺序提供载荷分量。
    /// 前缀、目标 ID 和幂等键不得为空白；目标 ID 保留原文，幂等键裁剪后只保存摘要。
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

        assemble(
            prefix,
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

    /// 取领域写入回执及重试查证共用的稳定 ID，而非业务资源 ID。
    pub fn id(&self) -> &str {
        self.identity.current_id()
    }

    /// 取请求操作人 ID，用于持久化回执身份及核对重试是否来自同一操作人。
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    /// 取命令的业务动作，用于在回执中区分同一资源类型下的不同操作。
    pub fn action(&self) -> &str {
        &self.action
    }

    /// 取命令对应的业务资源类型，供回执入库和查证时核对身份范围。
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// 取资源定位命令的目标 ID，供回执核对原操作对象；[`Self::from_payload`] 构造的回执无目标定位。
    pub fn scope_id(&self) -> Option<&str> {
        self.scope_id.as_deref()
    }

    /// 为持久化身份提供规范化操作号的长度前缀 SHA-256 v1 摘要，不暴露原操作号。
    pub fn idempotency_key_hash(&self) -> &CommandFingerprint {
        &self.idempotency_key_hash
    }

    /// 取规范请求的版本化载荷指纹；回执身份匹配后，以它区分原请求重试与不同载荷冲突。
    pub fn fingerprint(&self) -> &CommandFingerprint {
        &self.fingerprint
    }
}

// 把可序列化请求转换为键有序的 JSON，使对象字段生成顺序不影响幂等指纹。
fn canonical_json<T: Serialize>(payload: &T) -> Result<String> {
    let value = serde_json::to_value(payload).map_err(payload_serialize_error)?;
    let mut output = String::new();
    write_canonical_json(&value, &mut output)?;
    Ok(output)
}

// 为规范 JSON 按 JSON 规则编码对象键或标量，并将失败归入命令载荷错误。
fn write_scalar_json(value: &impl Serialize, output: &mut String) -> Result<()> {
    let raw = serde_json::to_string(value).map_err(payload_serialize_error)?;
    output.push_str(&raw);
    Ok(())
}

// 计算指纹前递归按键排序对象，保留可能表达业务差异的数组次序；任一编码失败都向上返回。
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

    #[test]
    fn v1_receipts_preserve_persisted_ids_and_normalization() {
        let created = CommandReceipt::from_payload(
            "c-",
            "actor",
            "update",
            "object",
            " 操作号 ",
            &json!({"b":2,"a":1}),
        )
        .unwrap();
        let resource = CommandReceipt::from_resource_parts(
            "c-",
            "actor",
            "update",
            "object",
            " target ",
            " 操作号 ",
            [],
        )
        .unwrap();

        assert_eq!(created.id(), "c-df6c7183067dda8b7faa5210f5251b87985910a02357af149a2547f4eadad6d3");
        assert_eq!(resource.id(), "c-222a373cb9efe4ee3ec7afbeba9311a6955b4ccfd741fbd67adc59252866913c");
        assert_eq!(resource.scope_id(), Some(" target "));

        assert_eq!(
            created.fingerprint().digest_hex(),
            "123e70809840e65313a5aff730d1d8d7ac65940805b5f018e39af98c2e37dfb1"
        );
        assert_eq!(
            created.idempotency_key_hash().digest_hex(),
            "71187bbc6643d4da9e634287512267583de25642adf6156808e2e0edc00fc19b"
        );
        assert_eq!(created.idempotency_key_hash(), resource.idempotency_key_hash());
    }

    #[test]
    fn resource_receipt_consumes_fingerprint_parts_in_order() {
        let receipt = CommandReceipt::from_resource_parts(
            "c-",
            "actor",
            "update",
            "object",
            "target",
            "key",
            ["a", "bc"].into_iter().map(str::to_owned),
        )
        .unwrap();

        assert_eq!(receipt.fingerprint(), &CommandFingerprint::from_parts(["a".into(), "bc".into()]));
        assert_ne!(receipt.fingerprint(), &CommandFingerprint::from_parts(["ab".into(), "c".into()]));
    }
}
