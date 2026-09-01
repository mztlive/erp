//! 命令幂等收据。相同键同载荷回读，不同载荷冲突。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{
    de::{Error as DeserializeError, IgnoredAny, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::fmt;

use crate::ids::ApprovalCommandReceiptId;
use crate::model::types::{
    base_model_at, normalize_required, ApprovalCommandKind, ModelError, ModelResult, SCOPE_MAX_LEN,
};
use crate::model::{ApprovalCommandIdentity, IdempotencyKey, Timestamp};

/// 命令执行收据。
#[derive(Debug, Serialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 命令种类。
    pub command_kind: ApprovalCommandKind,
    /// 作用域 ID。
    pub scope_id: String,
    /// 调用方幂等键。
    pub idempotency_key: IdempotencyKey,
    /// 规范化请求摘要。
    pub payload_digest: String,
    /// 不可变结果引用。
    pub result_ref: String,
}

impl<'de> Deserialize<'de> for ApprovalCommandReceipt {
    /// 兼容历史 writer 可能写出的两个同名 `created_at`。
    ///
    /// 历史模型同时展开 `BaseModel.created_at` 并显式序列化 `created_at`；MongoDB
    /// 驱动直接序列化实体时会保留两个 BSON 键。两个值相同时按一个创建时间读取；
    /// 值不一致时失败关闭，禁止任意选择一个时间掩盖损坏事实。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ReceiptVisitor;

        impl<'de> Visitor<'de> for ReceiptVisitor {
            type Value = ApprovalCommandReceipt;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an approval command receipt")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut id = None;
                let mut version = None;
                let mut created_at = None;
                let mut created_at_count = 0_u8;
                let mut updated_at = None;
                let mut deleted_at = None;
                let mut command_kind = None;
                let mut scope_id = None;
                let mut idempotency_key = None;
                let mut payload_digest = None;
                let mut result_ref = None;

                while let Some(field) = map.next_key::<String>()? {
                    match field.as_str() {
                        "id" => {
                            if id.is_some() {
                                return Err(A::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        "version" => {
                            if version.is_some() {
                                return Err(A::Error::duplicate_field("version"));
                            }
                            version = Some(map.next_value()?);
                        }
                        "created_at" => {
                            created_at_count = created_at_count.saturating_add(1);
                            if created_at_count > 2 {
                                return Err(A::Error::custom(
                                    "created_at may appear at most twice for legacy receipts",
                                ));
                            }
                            let value = map.next_value()?;
                            match created_at {
                                Some(existing) if existing != value => {
                                    return Err(A::Error::custom("duplicate created_at values do not match"));
                                }
                                Some(_) => {}
                                None => created_at = Some(value),
                            }
                        }
                        "updated_at" => {
                            if updated_at.is_some() {
                                return Err(A::Error::duplicate_field("updated_at"));
                            }
                            updated_at = Some(map.next_value()?);
                        }
                        "deleted_at" => {
                            if deleted_at.is_some() {
                                return Err(A::Error::duplicate_field("deleted_at"));
                            }
                            deleted_at = Some(map.next_value()?);
                        }
                        "command_kind" => {
                            if command_kind.is_some() {
                                return Err(A::Error::duplicate_field("command_kind"));
                            }
                            command_kind = Some(map.next_value()?);
                        }
                        "scope_id" => {
                            if scope_id.is_some() {
                                return Err(A::Error::duplicate_field("scope_id"));
                            }
                            scope_id = Some(map.next_value()?);
                        }
                        "idempotency_key" => {
                            if idempotency_key.is_some() {
                                return Err(A::Error::duplicate_field("idempotency_key"));
                            }
                            idempotency_key = Some(map.next_value()?);
                        }
                        "payload_digest" => {
                            if payload_digest.is_some() {
                                return Err(A::Error::duplicate_field("payload_digest"));
                            }
                            payload_digest = Some(map.next_value()?);
                        }
                        "result_ref" => {
                            if result_ref.is_some() {
                                return Err(A::Error::duplicate_field("result_ref"));
                            }
                            result_ref = Some(map.next_value()?);
                        }
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }

                Ok(ApprovalCommandReceipt {
                    base: BaseModel {
                        id: id.ok_or_else(|| A::Error::missing_field("id"))?,
                        version: version.ok_or_else(|| A::Error::missing_field("version"))?,
                        created_at: created_at.ok_or_else(|| A::Error::missing_field("created_at"))?,
                        updated_at: updated_at.ok_or_else(|| A::Error::missing_field("updated_at"))?,
                        deleted_at: deleted_at.ok_or_else(|| A::Error::missing_field("deleted_at"))?,
                    },
                    command_kind: command_kind.ok_or_else(|| A::Error::missing_field("command_kind"))?,
                    scope_id: scope_id.ok_or_else(|| A::Error::missing_field("scope_id"))?,
                    idempotency_key: idempotency_key
                        .ok_or_else(|| A::Error::missing_field("idempotency_key"))?,
                    payload_digest: payload_digest
                        .ok_or_else(|| A::Error::missing_field("payload_digest"))?,
                    result_ref: result_ref.ok_or_else(|| A::Error::missing_field("result_ref"))?,
                })
            }
        }

        deserializer.deserialize_map(ReceiptVisitor)
    }
}

impl ApprovalCommandReceipt {
    /// 创建命令收据。
    ///
    /// # 参数
    /// * `id` - 收据主键
    /// * `identity` - 已规范化的 v3 命令身份
    /// * `result_ref` - 结果引用
    /// * `at` - 创建时间
    ///
    /// # 错误
    /// 结果引用非法或调用方时间无法持久化时返回错误。
    pub fn new(
        id: ApprovalCommandReceiptId,
        identity: &ApprovalCommandIdentity,
        result_ref: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        Ok(Self {
            base: base_model_at(id.to_string(), at)?,
            command_kind: identity.command_kind(),
            scope_id: identity.scope().as_str().to_string(),
            idempotency_key: identity.idempotency_key().clone(),
            payload_digest: identity.digest().as_str().to_string(),
            result_ref: normalize_required(result_ref, "结果引用不能为空", SCOPE_MAX_LEN, "结果引用过长")?,
        })
    }

    /// 按完整 v3 身份回读：唯一键身份与载荷均相同才返回自身。
    ///
    /// # 错误
    /// 命令种类、作用域、规范幂等键或摘要任一不一致时返回
    /// [`ModelError::CommandReceiptConflict`]。
    pub fn reconcile_identity(&self, identity: &ApprovalCommandIdentity) -> ModelResult<&Self> {
        if self.command_kind == identity.command_kind()
            && self.scope_id == identity.scope().as_str()
            && &self.idempotency_key == identity.idempotency_key()
        {
            return self.reconcile(identity.digest().as_str());
        }
        Err(ModelError::CommandReceiptConflict)
    }

    /// 按相同键回读：摘要相同返回自身，摘要不同冲突。
    ///
    /// # 参数
    /// * `payload_digest` - 本次请求摘要
    ///
    /// # 返回
    /// 摘要相同时返回本收据。
    ///
    /// # 错误
    /// 摘要不同返回 [`ModelError::CommandReceiptConflict`]。
    pub fn reconcile(&self, payload_digest: &str) -> ModelResult<&Self> {
        if self.payload_digest == payload_digest {
            return Ok(self);
        }
        Err(ModelError::CommandReceiptConflict)
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalCommandReceipt;
    use crate::ids::ApprovalCommandReceiptId;
    use crate::model::types::{ApprovalCommandKind, ModelError};
    use crate::model::{
        ApprovalCommandIdentity, CanonicalCommandPayload, CommandPayloadField, IdempotencyKey, Timestamp,
    };
    use entity_core::BaseModel;
    use serde::Serialize;

    #[derive(Serialize)]
    struct LegacyApprovalCommandReceipt {
        #[serde(flatten)]
        base: BaseModel,
        command_kind: ApprovalCommandKind,
        scope_id: String,
        idempotency_key: String,
        payload_digest: String,
        result_ref: String,
        created_at: Timestamp,
    }

    #[derive(Serialize)]
    struct CorruptApprovalCommandReceipt {
        #[serde(flatten)]
        legacy: LegacyApprovalCommandReceipt,
        created_at: Timestamp,
    }
    use bson;

    fn receipt() -> ApprovalCommandReceipt {
        let identity = identity("digest-a");
        ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            &identity,
            "exec-1",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    fn identity(payload: &str) -> ApprovalCommandIdentity {
        ApprovalCommandIdentity::new(
            ApprovalCommandKind::SubmitDecision,
            "approval.submit-decision",
            IdempotencyKey::parse("key-1").unwrap(),
            CanonicalCommandPayload::new().field(CommandPayloadField::Text("inst-1")),
            CanonicalCommandPayload::new().field(CommandPayloadField::Text(payload)),
        )
        .unwrap()
    }

    /// 同载荷回读，不同载荷冲突。
    #[test]
    fn same_key_same_payload_reads_back() {
        let receipt = receipt();
        let same = identity("digest-a");
        let changed = identity("digest-b");
        assert!(receipt.reconcile_identity(&same).is_ok());
        assert_eq!(
            receipt.reconcile_identity(&changed),
            Err(ModelError::CommandReceiptConflict)
        );
    }

    /// 收据只使用 BaseModel 的创建时间；BSON 不得出现同名字段反序列化冲突。
    #[test]
    fn bson_round_trip_has_single_created_at_field() {
        let receipt = receipt();
        let document = bson::serialize_to_document(&receipt).expect("命令收据必须可序列化");
        assert_eq!(document.keys().filter(|key| *key == "created_at").count(), 1);
        let decoded: ApprovalCommandReceipt = bson::deserialize_from_document(document)
            .expect("历史单 created_at 字段必须由 BaseModel 唯一读取");
        assert_eq!(decoded, receipt);
        assert_eq!(decoded.base.created_at, 1);
    }

    /// 历史 Mongo raw writer 保留的双创建时间同值时必须继续可读。
    #[test]
    fn raw_bson_reads_matching_legacy_duplicate_created_at_fields() {
        let receipt = receipt();
        let legacy = LegacyApprovalCommandReceipt {
            base: receipt.base.clone(),
            command_kind: receipt.command_kind,
            scope_id: receipt.scope_id.clone(),
            idempotency_key: receipt.idempotency_key.to_string(),
            payload_digest: receipt.payload_digest.clone(),
            result_ref: receipt.result_ref.clone(),
            created_at: Timestamp::from_unix_secs(1).unwrap(),
        };
        let raw =
            bson::serialize_to_raw_document_buf(&legacy).expect("历史命令收据必须按 Mongo raw writer 序列化");
        let created_at_count = raw
            .iter()
            .map(|entry| entry.expect("raw BSON 字段必须有效"))
            .filter(|(key, _)| *key == "created_at")
            .count();
        assert_eq!(created_at_count, 2, "fixture 必须真实包含两个同名 BSON 键");

        let decoded: ApprovalCommandReceipt =
            bson::deserialize_from_slice(raw.as_bytes()).expect("同值的历史双 created_at 必须兼容读取");
        assert_eq!(decoded, receipt);
    }

    /// 历史双创建时间若不一致必须失败关闭，禁止任意覆盖。
    #[test]
    fn raw_bson_rejects_conflicting_legacy_duplicate_created_at_fields() {
        let receipt = receipt();
        let legacy = LegacyApprovalCommandReceipt {
            base: receipt.base.clone(),
            command_kind: receipt.command_kind,
            scope_id: receipt.scope_id,
            idempotency_key: receipt.idempotency_key.to_string(),
            payload_digest: receipt.payload_digest,
            result_ref: receipt.result_ref,
            created_at: Timestamp::from_unix_secs(2).unwrap(),
        };
        let raw =
            bson::serialize_to_raw_document_buf(&legacy).expect("损坏历史命令收据 fixture 必须可序列化");

        let error = bson::deserialize_from_slice::<ApprovalCommandReceipt>(raw.as_bytes())
            .expect_err("不一致的历史双 created_at 必须失败关闭");
        assert!(
            error
                .to_string()
                .contains("duplicate created_at values do not match"),
            "unexpected error: {error}"
        );
    }

    /// 历史合法形态最多两个创建时间；第三个同值键也属于损坏文档。
    #[test]
    fn raw_bson_rejects_more_than_two_created_at_fields() {
        let receipt = receipt();
        let legacy = LegacyApprovalCommandReceipt {
            base: receipt.base,
            command_kind: receipt.command_kind,
            scope_id: receipt.scope_id,
            idempotency_key: receipt.idempotency_key.to_string(),
            payload_digest: receipt.payload_digest,
            result_ref: receipt.result_ref,
            created_at: Timestamp::from_unix_secs(1).unwrap(),
        };
        let corrupt = CorruptApprovalCommandReceipt {
            legacy,
            created_at: Timestamp::from_unix_secs(1).unwrap(),
        };
        let raw =
            bson::serialize_to_raw_document_buf(&corrupt).expect("三 created_at 损坏 fixture 必须可序列化");

        let error = bson::deserialize_from_slice::<ApprovalCommandReceipt>(raw.as_bytes())
            .expect_err("第三个 created_at 必须失败关闭");
        assert!(
            error
                .to_string()
                .contains("created_at may appear at most twice for legacy receipts"),
            "unexpected error: {error}"
        );
    }

    /// 历史 scope/digest 无版本字符串继续只读兼容，幂等键仍须是规范形态。
    #[test]
    fn legacy_scope_and_digest_strings_remain_readable() {
        let base = receipt().base;
        let legacy = LegacyApprovalCommandReceipt {
            base,
            command_kind: ApprovalCommandKind::SubmitDecision,
            scope_id: "inst-legacy".to_string(),
            idempotency_key: "key-legacy".to_string(),
            payload_digest: "legacy-digest".to_string(),
            result_ref: "exec-legacy".to_string(),
            created_at: Timestamp::from_unix_secs(1).unwrap(),
        };
        let raw = bson::serialize_to_raw_document_buf(&legacy).expect("历史收据必须可序列化");
        let decoded: ApprovalCommandReceipt =
            bson::deserialize_from_slice(raw.as_bytes()).expect("历史 scope/digest 必须继续可读");

        assert_eq!(decoded.scope_id, "inst-legacy");
        assert_eq!(decoded.payload_digest, "legacy-digest");
        assert_eq!(decoded.idempotency_key.as_str(), "key-legacy");
    }
}
