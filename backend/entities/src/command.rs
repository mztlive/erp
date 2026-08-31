//! 业务命令共享的版本化稳定指纹基元。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{Error, Result};

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
    pub fn from_parts(parts: impl IntoIterator<Item = String>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"command-fingerprint-v1");
        for part in parts {
            hasher.update((part.len() as u64).to_be_bytes());
            hasher.update(part.as_bytes());
        }
        Self(format!("{V1_PREFIX}{}", hex::encode(hasher.finalize())))
    }

    /// 解析已持久化的 v1 指纹。
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let digest = value
            .strip_prefix(V1_PREFIX)
            .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| Error::from("命令指纹格式无效"))?;
        Ok(Self(format!("{V1_PREFIX}{}", digest.to_ascii_lowercase())))
    }

    /// 返回稳定持久化字符串。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 返回 64 位摘要部分。
    pub fn digest_hex(&self) -> &str {
        self.0.strip_prefix(V1_PREFIX).expect("已验证指纹必须有 v1 前缀")
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
        let mut legacy_ids = legacy_ids
            .into_iter()
            .filter(|id| id != &current_id)
            .collect::<Vec<_>>();
        legacy_ids.sort();
        legacy_ids.dedup();
        Ok(Self {
            current_id,
            legacy_ids,
        })
    }

    /// 返回新写入使用的 v1 ID。
    pub fn current_id(&self) -> &str {
        &self.current_id
    }

    /// 返回按当前优先、历史其次排列的查询候选 ID。
    pub fn candidates(&self) -> Vec<String> {
        std::iter::once(self.current_id.clone())
            .chain(self.legacy_ids.iter().cloned())
            .collect()
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

impl CommandReceipt {
    /// 从可序列化请求形成规范 JSON v1 收据，并保留旧 JSON/摘要兼容候选。
    pub fn from_payload<T: Serialize>(
        prefix: &str,
        actor_id: &str,
        action: &str,
        resource_type: &str,
        idempotency_key: &str,
        payload: &T,
    ) -> Result<Self> {
        let key = idempotency_key.trim();
        if key.is_empty() {
            return Err(Error::from("操作号不能为空"));
        }
        let canonical_payload = canonical_json(payload)?;
        let legacy_payload = serde_json::to_string(payload)
            .map_err(|error| Error::from(format!("业务命令请求序列化失败: {error}")))?;
        let legacy_identity = format!(
            "{prefix}{}",
            legacy_digest_parts(&[actor_id, action, resource_type, key])
        );
        let identity = CommandIdentity::new(
            prefix,
            [actor_id, action, resource_type, key]
                .into_iter()
                .map(str::to_string),
            [legacy_identity],
        )?;
        let fingerprint = CommandFingerprint::from_parts([
            action.to_string(),
            resource_type.to_string(),
            canonical_payload,
        ]);
        let legacy_fingerprints = vec![legacy_digest_parts(&[action, resource_type, &legacy_payload])];
        Ok(Self {
            identity,
            actor_id: actor_id.to_string(),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            fingerprint,
            legacy_fingerprints,
        })
    }

    /// 从资源定位命令的固定顺序字段形成 v1 收据。
    ///
    /// 兼容候选使用历史 `actor|action|resource_id|key` 身份和无版本
    /// 长度前缀指纹；新写入不保存原始幂等键。
    pub fn from_resource_parts(
        prefix: &str,
        actor_id: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        idempotency_key: &str,
        fingerprint_parts: impl IntoIterator<Item = String>,
    ) -> Result<Self> {
        let key = idempotency_key.trim();
        if key.is_empty() {
            return Err(Error::from("操作号不能为空"));
        }
        if resource_id.trim().is_empty() {
            return Err(Error::from("命令资源 ID 不能为空"));
        }
        let fingerprint_parts = fingerprint_parts.into_iter().collect::<Vec<_>>();
        let legacy_identity = format!(
            "{prefix}{}",
            hex::encode(Sha256::digest(
                format!("{actor_id}|{action}|{resource_id}|{key}").as_bytes()
            ))
        );
        let legacy_fingerprint_parts = fingerprint_parts.iter().map(String::as_str).collect::<Vec<_>>();
        let legacy_fingerprint = legacy_digest_parts(&legacy_fingerprint_parts);
        let identity = CommandIdentity::new(
            prefix,
            [actor_id, action, resource_type, resource_id, key]
                .into_iter()
                .map(str::to_string),
            [legacy_identity],
        )?;
        Ok(Self {
            identity,
            actor_id: actor_id.to_string(),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            fingerprint: CommandFingerprint::from_parts(fingerprint_parts),
            legacy_fingerprints: vec![legacy_fingerprint],
        })
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
    pub fn message(&self, detail: Option<&str>) -> String {
        let base = format!("command_fingerprint={}", self.fingerprint.as_str());
        detail.map_or(base.clone(), |detail| format!("{base}; {detail}"))
    }

    /// 校验最小持久化事实并分类回放结果。
    pub fn match_fact(&self, fact: &CommandReceiptFact) -> CommandReceiptMatch {
        if !self.identity.candidates().contains(&fact.id)
            || !fact.success
            || fact.actor_id != self.actor_id
            || fact.action != self.action
            || fact.resource_type != self.resource_type
        {
            return CommandReceiptMatch::Corrupted;
        }
        let Some(message) = fact.message.as_deref() else {
            return CommandReceiptMatch::Corrupted;
        };
        let persisted = message.split(';').next().unwrap_or(message);
        let matches = persisted
            .strip_prefix("command_fingerprint=")
            .is_some_and(|value| value == self.fingerprint.as_str())
            || persisted
                .strip_prefix("command_sha256=")
                .is_some_and(|value| self.legacy_fingerprints.iter().any(|legacy| legacy == value));
        if !matches {
            return CommandReceiptMatch::DifferentPayload;
        }
        fact.resource_id
            .clone()
            .map(CommandReceiptMatch::SamePayload)
            .unwrap_or(CommandReceiptMatch::Corrupted)
    }
}

fn canonical_json<T: Serialize>(payload: &T) -> Result<String> {
    let value = serde_json::to_value(payload)
        .map_err(|error| Error::from(format!("业务命令请求序列化失败: {error}")))?;
    let mut output = String::new();
    write_canonical_json(&value, &mut output)?;
    Ok(output)
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
                output.push_str(&serde_json::to_string(key).map_err(|error| Error::from(error.to_string()))?);
                output.push(':');
                write_canonical_json(&map[key], output)?;
            }
            output.push('}');
        }
        serde_json::Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical_json(value, output)?;
            }
            output.push(']');
        }
        value => {
            output.push_str(&serde_json::to_string(value).map_err(|error| Error::from(error.to_string()))?)
        }
    }
    Ok(())
}

fn legacy_digest_parts(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use sha2::{Digest, Sha256};

    use super::{
        legacy_digest_parts, CommandFingerprint, CommandReceipt, CommandReceiptFact, CommandReceiptMatch,
    };

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
        assert_eq!(
            CommandFingerprint::parse(fingerprint.as_str()).unwrap(),
            fingerprint
        );
    }

    #[test]
    fn receipt_hides_raw_key_and_rejects_different_payload() {
        let first = Payload {
            amount: 100,
            idempotency_key: "secret-operation-key".to_string(),
        };
        let changed = Payload {
            amount: 200,
            idempotency_key: first.idempotency_key.clone(),
        };
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
        let fact = CommandReceiptFact {
            id: receipt.id().to_string(),
            actor_id: "actor-1".to_string(),
            action: "payment.commit".to_string(),
            resource_type: "payment".to_string(),
            resource_id: Some("payment-1".to_string()),
            success: true,
            message: Some(receipt.message(None)),
        };
        assert_eq!(
            receipt.match_fact(&fact),
            CommandReceiptMatch::SamePayload("payment-1".to_string())
        );
        let changed_receipt = CommandReceipt::from_payload(
            "receipt-",
            "actor-1",
            "payment.commit",
            "payment",
            &changed.idempotency_key,
            &changed,
        )
        .unwrap();
        assert_eq!(
            changed_receipt.match_fact(&fact),
            CommandReceiptMatch::DifferentPayload
        );
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
        let payload = Payload {
            amount: 100,
            idempotency_key: "legacy-key".to_string(),
        };
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
        let fact = CommandReceiptFact {
            id: legacy_id.clone(),
            actor_id: "actor-1".to_string(),
            action: "payment.commit".to_string(),
            resource_type: "payment".to_string(),
            resource_id: Some("payment-1".to_string()),
            success: true,
            message: Some(format!("command_sha256={legacy_fingerprint}")),
        };
        assert!(receipt.id_candidates().contains(&legacy_id));
        assert_eq!(
            receipt.match_fact(&fact),
            CommandReceiptMatch::SamePayload("payment-1".to_string())
        );
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
        let fact = CommandReceiptFact {
            id: legacy_id.clone(),
            actor_id: "actor-1".to_string(),
            action: "work_item.reassign".to_string(),
            resource_type: "work_item".to_string(),
            resource_id: Some("wi-1".to_string()),
            success: true,
            message: Some(format!("command_sha256={legacy_fingerprint}; reason=safe")),
        };
        assert!(receipt.id_candidates().contains(&legacy_id));
        assert_eq!(
            receipt.match_fact(&fact),
            CommandReceiptMatch::SamePayload("wi-1".to_string())
        );
        assert_ne!(
            CommandFingerprint::from_parts(["ab".to_string(), "c".to_string()]),
            CommandFingerprint::from_parts(["a".to_string(), "bc".to_string()])
        );
    }
}
