//! 供应商供给写命令的幂等结果。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
const OPERATION_MAX_LEN: usize = 64;
const FINGERPRINT_MAX_LEN: usize = 128;
/// 未来版本化指纹前缀；存量命令为历史裸摘要，两种形态按摘要兼容比较。
const FINGERPRINT_V1_PREFIX: &str = "sha256-v1:";
/// 历史裸摘要长度（64 位十六进制）。
const FINGERPRINT_HEX_LEN: usize = 64;

/// 已成功供给写命令的持久化数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOfferingCommandData {
    /// 客户端生成的幂等键。
    pub idempotency_key: String,
    /// 稳定操作名。
    pub operation: String,
    /// 包含路径目标与请求体的 SHA-256 指纹。
    pub request_fingerprint: String,
    /// 序列化后的稳定响应结果。
    pub result_json: String,
}

/// 供给命令去重记录；必须与业务写入处于同一事务。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierOfferingCommand {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 客户端生成的幂等键。
    pub idempotency_key: String,
    /// 稳定操作名。
    pub operation: String,
    /// 原始请求指纹。
    pub request_fingerprint: String,
    /// 原始成功响应。
    pub result_json: String,
}

impl SupplierOfferingCommand {
    /// 创建已成功命令的去重记录。
    ///
    /// # 参数
    /// * `id` - 命令主键
    /// * `data` - 命令内容
    ///
    /// # 返回
    /// 返回规范化后的命令记录。
    ///
    /// # 错误
    /// 字段为空或超过合同长度、请求指纹格式非法时返回错误。
    pub fn new(id: impl Into<String>, data: SupplierOfferingCommandData) -> Result<Self> {
        let idempotency_key = normalize_required_text(
            data.idempotency_key,
            "幂等键不能为空",
            IDEMPOTENCY_KEY_MAX_LEN,
            "幂等键过长",
        )?;
        let operation = normalize_required_text(
            data.operation,
            "命令操作不能为空",
            OPERATION_MAX_LEN,
            "命令操作过长",
        )?;
        let request_fingerprint = normalize_required_text(
            data.request_fingerprint,
            "请求指纹不能为空",
            FINGERPRINT_MAX_LEN,
            "请求指纹过长",
        )?;
        validate_fingerprint_format(&request_fingerprint)?;
        if data.result_json.trim().is_empty() {
            return Err(Error::from("命令结果不能为空"));
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            idempotency_key,
            operation,
            request_fingerprint,
            result_json: data.result_json,
        })
    }

    /// 创建带序列化成功结果的命令去重记录。
    ///
    /// # 参数
    /// * `id` - 命令主键
    /// * `idempotency_key` - 客户端生成的幂等键
    /// * `operation` - 稳定操作名
    /// * `request_fingerprint` - 当前请求指纹（接受 `sha256-v1:<hex>` 或历史裸 `64hex`）
    /// * `result` - 与请求同构的成功响应
    ///
    /// # 返回
    /// 返回把 `result` 序列化为 `result_json` 后的规范化命令记录。
    ///
    /// # 错误
    /// 字段为空或超过合同长度、指纹格式非法或结果序列化失败时返回错误。
    ///
    /// # 约束
    /// 纯内存序列化与规范化，不触及 MongoDB 或时钟；命令主键由 Service 注入。
    pub fn with_result<T: Serialize>(
        id: impl Into<String>,
        idempotency_key: impl Into<String>,
        operation: impl Into<String>,
        request_fingerprint: impl Into<String>,
        result: &T,
    ) -> Result<Self> {
        let result_json = serde_json::to_string(result).map_err(|_| Error::from("供给命令结果序列化失败"))?;
        Self::new(
            id,
            SupplierOfferingCommandData {
                idempotency_key: idempotency_key.into(),
                operation: operation.into(),
                request_fingerprint: request_fingerprint.into(),
                result_json,
            },
        )
    }

    /// 校验幂等重放的操作与指纹一致性。
    ///
    /// # 参数
    /// * `operation` - 期望操作名（如 `create_offering`）
    /// * `request_fingerprint` - 当前请求指纹（接受 `sha256-v1:<hex>` 或历史裸 `64hex`）
    ///
    /// # 返回
    /// 操作与指纹均一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 操作或指纹任一不一致时返回校验错误。
    ///
    /// # 约束
    /// 纯内存校验，不触及 MongoDB 或时钟；指纹按摘要兼容比较（前缀与大小写不敏感），
    /// 存量裸摘要命令可继续重放。
    pub fn ensure_replayable(&self, operation: &str, request_fingerprint: &str) -> Result<()> {
        if self.operation != operation || !fingerprints_match(&self.request_fingerprint, request_fingerprint)
        {
            return Err(Error::from("幂等键已用于不同的供给命令"));
        }
        Ok(())
    }

    /// 解码已持久化的成功响应结果。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与存储 JSON 同构的强类型结果。
    ///
    /// # 错误
    /// 存储结果 JSON 非法时返回稳定的内部错误。
    ///
    /// # 约束
    /// 纯内存反序列化，不触及 MongoDB；结果类型由调用方（Service View）指定。
    pub fn replay_result<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_str(&self.result_json).map_err(|_| Error::from("供给命令结果反序列化失败"))
    }
}

/// 提取指纹摘要部分，兼容 `sha256-v1:` 前缀与历史裸摘要。
///
/// # 参数
/// * `value` - 完整指纹字符串
///
/// # 返回
/// 返回小写摘要切片；非法格式由外层校验失败。
fn fingerprint_digest(value: &str) -> &str {
    value.strip_prefix(FINGERPRINT_V1_PREFIX).unwrap_or(value)
}

/// 校验指纹格式：接受 `sha256-v1:<64hex>` 或历史裸 `64hex`。
///
/// # 参数
/// * `value` - 待校验指纹
///
/// # 返回
/// 格式合法返回 `Ok(())`。
///
/// # 错误
/// 非法长度、非十六进制或空白时返回错误。
fn validate_fingerprint_format(value: &str) -> Result<()> {
    let digest = fingerprint_digest(value.trim());
    if digest.len() != FINGERPRINT_HEX_LEN
        || !digest.bytes().all(|b| b.is_ascii_hexdigit())
        || value.trim() != value
    {
        return Err(Error::from("供给命令请求指纹格式无效"));
    }
    Ok(())
}

/// 指纹按摘要兼容比较，前缀与大小写不敏感。
///
/// # 参数
/// * `a` - 已持久化指纹
/// * `b` - 当前请求指纹
///
/// # 返回
/// 摘要一致返回 `true`。
fn fingerprints_match(a: &str, b: &str) -> bool {
    fingerprint_digest(a).eq_ignore_ascii_case(fingerprint_digest(b))
        && validate_fingerprint_format(a).is_ok()
        && validate_fingerprint_format(b).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{SupplierOfferingCommand, SupplierOfferingCommandData, FINGERPRINT_V1_PREFIX};
    use serde::Serialize;

    const ZERO_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const ONE_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn command_normalizes_identity_and_rejects_empty_result() {
        let command = SupplierOfferingCommand::new(
            "command-1",
            SupplierOfferingCommandData {
                idempotency_key: " key-1 ".to_string(),
                operation: " create_offering ".to_string(),
                request_fingerprint: format!(" {ZERO_DIGEST} "),
                result_json: "{\"offering_id\":\"o1\"}".to_string(),
            },
        )
        .unwrap();
        assert_eq!(command.idempotency_key, "key-1");
        assert_eq!(command.operation, "create_offering");
        assert_eq!(command.request_fingerprint, ZERO_DIGEST);

        let invalid = SupplierOfferingCommandData {
            idempotency_key: "key-2".to_string(),
            operation: "create_offering".to_string(),
            request_fingerprint: ZERO_DIGEST.to_string(),
            result_json: " ".to_string(),
        };
        assert!(SupplierOfferingCommand::new("command-2", invalid).is_err());
    }

    /// 覆盖重放一致性：同键同操作同载荷可重放，操作或指纹任一不同必须冲突。
    #[test]
    fn replay_is_bound_to_operation_and_fingerprint() {
        let command = command_with(ZERO_DIGEST);
        assert!(command.ensure_replayable("create_offering", ZERO_DIGEST).is_ok());
        assert!(command.ensure_replayable("revise_offering", ZERO_DIGEST).is_err());
        assert!(command.ensure_replayable("create_offering", ONE_DIGEST).is_err());
        assert!(command
            .ensure_replayable("create_offering", "bad-fingerprint")
            .is_err());
    }

    /// 覆盖指纹版本化与历史裸摘要兼容：前带 `sha256-v1:` 与裸摘要按摘要兼容比较。
    #[test]
    fn fingerprint_versioned_and_legacy_are_compatible() {
        let bare = command_with(ZERO_DIGEST);
        let versioned = SupplierOfferingCommand::new(
            "cmd-versioned",
            SupplierOfferingCommandData {
                request_fingerprint: format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}"),
                ..command_data(ZERO_DIGEST)
            },
        )
        .unwrap();
        assert!(bare.ensure_replayable("create_offering", ZERO_DIGEST).is_ok());
        assert!(bare
            .ensure_replayable(
                "create_offering",
                &format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}")
            )
            .is_ok());
        assert!(versioned
            .ensure_replayable("create_offering", ZERO_DIGEST)
            .is_ok());
        assert!(versioned
            .ensure_replayable("create_offering", ONE_DIGEST)
            .is_err());
        let doc = bson::serialize_to_document(&bare).unwrap();
        let roundtrip: SupplierOfferingCommand = bson::deserialize_from_document(doc).unwrap();
        assert_eq!(roundtrip.request_fingerprint, ZERO_DIGEST);
    }

    /// 覆盖指纹格式校验：非法长度、非十六进制或空白拒绝。
    #[test]
    fn fingerprint_format_rejects_invalid() {
        let bad = SupplierOfferingCommandData {
            request_fingerprint: "not-a-hex".to_string(),
            ..command_data(ZERO_DIGEST)
        };
        assert!(SupplierOfferingCommand::new("bad-1", bad).is_err());
        let short = SupplierOfferingCommandData {
            request_fingerprint: ZERO_DIGEST[..32].to_string(),
            ..command_data(ZERO_DIGEST)
        };
        assert!(SupplierOfferingCommand::new("bad-2", short).is_err());
    }

    /// 覆盖结果序列化与重放解码：写入的 `result_json` 与输入同构，解码后可完整还原。
    #[test]
    fn with_result_serializes_and_replays_roundtrip() {
        #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
        struct OfferingResult {
            offering_id: String,
            revision_no: u32,
        }
        let expected = OfferingResult {
            offering_id: "offering-1".to_string(),
            revision_no: 2,
        };
        let command = SupplierOfferingCommand::with_result(
            "command-1",
            "key-1",
            "revise_offering",
            ZERO_DIGEST,
            &expected,
        )
        .unwrap();
        assert_eq!(
            command.result_json,
            "{\"offering_id\":\"offering-1\",\"revision_no\":2}"
        );
        assert_eq!(command.replay_result::<OfferingResult>().unwrap(), expected);
    }

    /// 覆盖坏结果 JSON：存储结果非法时返回稳定内部错误而非 panic。
    #[test]
    fn replay_result_rejects_bad_json_with_stable_error() {
        let command = SupplierOfferingCommand::new(
            "command-1",
            SupplierOfferingCommandData {
                idempotency_key: "key-1".to_string(),
                operation: "create_offering".to_string(),
                request_fingerprint: ZERO_DIGEST.to_string(),
                result_json: "not-json".to_string(),
            },
        )
        .unwrap();
        let error = command
            .replay_result::<serde_json::Value>()
            .expect_err("坏结果 JSON 必须失败");
        assert_eq!(error.to_string(), "供给命令结果反序列化失败");
    }

    /// 覆盖结果序列化失败：不可序列化结果返回稳定错误，不生成部分命令记录。
    #[test]
    fn with_result_serialization_failure_is_stable() {
        struct FailingSerialize;
        impl Serialize for FailingSerialize {
            fn serialize<S: serde::Serializer>(
                &self,
                _serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("测试序列化失败"))
            }
        }
        let error = SupplierOfferingCommand::with_result(
            "command-1",
            "key-1",
            "create_offering",
            ZERO_DIGEST,
            &FailingSerialize,
        )
        .expect_err("序列化失败必须返回错误");
        assert_eq!(error.to_string(), "供给命令结果序列化失败");
    }

    fn command_with(fingerprint: &str) -> SupplierOfferingCommand {
        SupplierOfferingCommand::new("command-1", command_data(fingerprint)).unwrap()
    }

    fn command_data(fingerprint: &str) -> SupplierOfferingCommandData {
        SupplierOfferingCommandData {
            idempotency_key: "key-1".to_string(),
            operation: "create_offering".to_string(),
            request_fingerprint: fingerprint.to_string(),
            result_json: "{\"offering_id\":\"o1\"}".to_string(),
        }
    }
}
