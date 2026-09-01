//! 供应商资料根级命令去重结果。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
/// 当前供应商资料请求指纹版本前缀（规范 JSON 的 SHA-256）。
const FINGERPRINT_V1_PREFIX: &str = "sha256-v1:";
/// 历史裸摘要长度（64 位十六进制）。
const FINGERPRINT_HEX_LEN: usize = 64;

/// 根级保存命令成功后持久化的稳定结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierProfileCommandData {
    /// 客户端生成的幂等键。
    pub idempotency_key: String,
    /// 创建或修订。
    pub operation: String,
    /// 原始命令的稳定 SHA-256 指纹，用于拒绝幂等键被不同请求复用。
    pub request_fingerprint: String,
    /// 供应商 ID。
    pub supplier_id: String,
    /// 供应商编号。
    pub supplier_no: String,
    /// 当前商务版本 ID。
    pub revision_id: String,
    /// 当前商务版本号。
    pub revision_no: u32,
    /// 保存后的供应商乐观锁版本。
    pub supplier_version: u64,
    /// 业务生效日期。
    pub effective_from: BusinessDate,
    /// 原始变更原因。
    pub change_reason: String,
}

/// 根级命令去重记录；与业务写入处于同一 MongoDB 事务。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierProfileCommand {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 客户端幂等键。
    pub idempotency_key: String,
    /// 操作类型。
    pub operation: String,
    /// 原始命令的稳定 SHA-256 指纹。
    #[serde(default)]
    pub request_fingerprint: String,
    /// 供应商 ID。
    pub supplier_id: String,
    /// 供应商编号。
    pub supplier_no: String,
    /// 商务版本 ID。
    pub revision_id: String,
    /// 商务版本号。
    pub revision_no: u32,
    /// 供应商乐观锁版本。
    pub supplier_version: u64,
    /// 业务生效日期。
    pub effective_from: BusinessDate,
    /// 原始变更原因。
    pub change_reason: String,
}

impl SupplierProfileCommand {
    /// 创建已成功根级命令的去重记录。
    ///
    /// # Errors
    /// 幂等键、结果身份、请求指纹或变更原因为空，或幂等键过长、指纹格式非法时返回校验错误。
    ///
    /// # 约束
    /// 请求指纹接受 `sha256-v1:<64hex>` 或历史裸 `64hex`；存储按传入原样保留，比较时按摘要兼容。
    pub fn new(id: impl Into<String>, data: SupplierProfileCommandData) -> Result<Self> {
        let idempotency_key = normalize_required_text(
            data.idempotency_key,
            "幂等键不能为空",
            IDEMPOTENCY_KEY_MAX_LEN,
            "幂等键过长",
        )?;
        if data.supplier_id.trim().is_empty()
            || data.supplier_no.trim().is_empty()
            || data.revision_id.trim().is_empty()
            || data.request_fingerprint.trim().is_empty()
            || data.change_reason.trim().is_empty()
        {
            return Err(Error::from("供应商命令结果身份、请求指纹或变更原因不能为空"));
        }
        validate_fingerprint_format(&data.request_fingerprint)?;
        Ok(Self {
            base: BaseModel::new(id.into()),
            idempotency_key,
            operation: data.operation,
            request_fingerprint: data.request_fingerprint,
            supplier_id: data.supplier_id,
            supplier_no: data.supplier_no,
            revision_id: data.revision_id,
            revision_no: data.revision_no,
            supplier_version: data.supplier_version,
            effective_from: data.effective_from,
            change_reason: data.change_reason,
        })
    }

    /// 校验幂等重放的操作、目标与指纹一致性。
    ///
    /// # 参数
    /// * `operation` - 期望操作类型（`create` 或 `update`）
    /// * `supplier_id` - 仅 `update` 时校验目标供应商 ID，`create` 传入 `None`
    /// * `request_fingerprint` - 当前请求指纹（接受 `sha256-v1:<hex>` 或历史裸 `64hex`）
    ///
    /// # 返回
    /// 操作、目标与指纹均一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 操作、指纹或目标任一不一致时返回校验错误。
    ///
    /// # 约束
    /// 纯内存校验，不触及 MongoDB 或时钟；指纹按摘要兼容比较（前缀与大小写不敏感）。
    pub fn ensure_replayable(
        &self,
        operation: &str,
        supplier_id: Option<&str>,
        request_fingerprint: &str,
    ) -> Result<()> {
        if self.operation != operation || !fingerprints_match(&self.request_fingerprint, request_fingerprint)
        {
            return Err(Error::from("幂等键已用于不同的供应商资料请求"));
        }
        if supplier_id.is_some_and(|expected| self.supplier_id != expected) {
            return Err(Error::from("幂等键已用于其他供应商命令"));
        }
        Ok(())
    }

    /// 校验乐观锁版本。
    ///
    /// # 参数
    /// * `actual` - 当前持久化版本
    /// * `expected` - 客户端期望版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 版本不一致时返回校验错误。
    ///
    /// # 约束
    /// 纯内存比较，不触及外部状态。
    pub fn ensure_version(actual: u64, expected: u64) -> Result<()> {
        if actual != expected {
            return Err(Error::from("数据已被其他请求修改，请刷新后重试"));
        }
        Ok(())
    }

    /// 读取更新场景必填版本号。
    ///
    /// # 参数
    /// * `value` - 可空版本输入
    /// * `object` - 业务对象名，用于错误文案
    ///
    /// # 返回
    /// 存在时返回版本号。
    ///
    /// # 错误
    /// `value` 为 `None` 时返回校验错误。
    ///
    /// # 约束
    /// 仅做存在性校验，不触及持久化。
    pub fn required_update_version(value: Option<u64>, object: &str) -> Result<u64> {
        value.ok_or_else(|| Error::from(format!("修订供应商时{object}版本不能为空")))
    }

    /// 校验创建场景必填的稳定业务编号。
    ///
    /// # 参数
    /// * `value` - 可空输入
    /// * `field` - 字段中文名，用于错误文案
    ///
    /// # 返回
    /// 去首尾空白后非空时返回规范化编号。
    ///
    /// # 错误
    /// 输入为空或全空白时返回校验错误。
    ///
    /// # 约束
    /// 仅做空白与存在性校验，不触及唯一性查询。
    pub fn required_create_identity(value: Option<&str>, field: &str) -> Result<String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| Error::from(format!("创建供应商时{field}不能为空")))
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
        return Err(Error::from("供应商资料请求指纹格式无效"));
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
    use crate::common::time::BusinessDate;

    use super::{SupplierProfileCommand, SupplierProfileCommandData, FINGERPRINT_V1_PREFIX};

    const ZERO_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const ONE_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn command_requires_idempotency_key_and_result_identity() {
        let data = SupplierProfileCommandData {
            idempotency_key: " supplier-save-1 ".to_string(),
            operation: "create".to_string(),
            request_fingerprint: ZERO_DIGEST.to_string(),
            supplier_id: "supplier-1".to_string(),
            supplier_no: "SUP-1".to_string(),
            revision_id: "revision-1".to_string(),
            revision_no: 1,
            supplier_version: 1,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            change_reason: "新建".to_string(),
        };
        let command = SupplierProfileCommand::new("command-1", data).unwrap();
        assert_eq!(command.idempotency_key, "supplier-save-1");

        let invalid = SupplierProfileCommandData {
            idempotency_key: " ".to_string(),
            ..command_data()
        };
        assert!(SupplierProfileCommand::new("command-2", invalid).is_err());
    }

    /// 覆盖重放一致性：同键同载荷可重放，操作、目标或指纹任一不同必须冲突。
    #[test]
    fn replay_is_bound_to_operation_target_and_fingerprint() {
        let command = SupplierProfileCommand::new("command-1", command_data()).unwrap();
        assert!(command
            .ensure_replayable("update", Some("supplier-1"), ZERO_DIGEST)
            .is_ok());
        assert!(command
            .ensure_replayable("create", Some("supplier-1"), ZERO_DIGEST)
            .is_err());
        assert!(command
            .ensure_replayable("update", Some("supplier-2"), ZERO_DIGEST)
            .is_err());
        assert!(command
            .ensure_replayable("update", Some("supplier-1"), ONE_DIGEST)
            .is_err());
        // create 场景的 supplier_id 为 None 时不校验目标
        let create = SupplierProfileCommand::new(
            "command-create",
            SupplierProfileCommandData {
                operation: "create".to_string(),
                ..command_data()
            },
        )
        .unwrap();
        assert!(create.ensure_replayable("create", None, ZERO_DIGEST).is_ok());
    }

    /// 覆盖指纹版本化与历史裸摘要兼容：前带 `sha256-v1:` 与裸摘要按摘要兼容。
    #[test]
    fn fingerprint_versioned_and_legacy_are_compatible() {
        let bare = SupplierProfileCommand::new("cmd-bare", bare_data(ZERO_DIGEST)).unwrap();
        let versioned = SupplierProfileCommand::new(
            "cmd-versioned",
            bare_data(&format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}")),
        )
        .unwrap();
        assert!(bare
            .ensure_replayable("update", Some("supplier-1"), ZERO_DIGEST)
            .is_ok());
        assert!(bare
            .ensure_replayable(
                "update",
                Some("supplier-1"),
                &format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}")
            )
            .is_ok());
        assert!(versioned
            .ensure_replayable("update", Some("supplier-1"), ZERO_DIGEST)
            .is_ok());
        assert!(versioned
            .ensure_replayable("update", Some("supplier-1"), ONE_DIGEST)
            .is_err());
        let doc = bson::serialize_to_document(&bare).unwrap();
        let roundtrip: SupplierProfileCommand = bson::deserialize_from_document(doc).unwrap();
        assert_eq!(roundtrip.request_fingerprint, ZERO_DIGEST);
        let doc2 = bson::serialize_to_document(&versioned).unwrap();
        let roundtrip2: SupplierProfileCommand = bson::deserialize_from_document(doc2).unwrap();
        assert_eq!(
            roundtrip2.request_fingerprint,
            format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}")
        );
    }

    /// 覆盖指纹格式校验：非法长度或非十六进制拒绝。
    #[test]
    fn fingerprint_format_rejects_invalid() {
        let bad = SupplierProfileCommandData {
            request_fingerprint: "not-a-hex".to_string(),
            ..command_data()
        };
        assert!(SupplierProfileCommand::new("bad-1", bad).is_err());
        let padded = SupplierProfileCommandData {
            request_fingerprint: format!(" {ZERO_DIGEST}"),
            ..command_data()
        };
        assert!(SupplierProfileCommand::new("bad-2", padded).is_err());
    }

    /// 覆盖版本与身份合同：版本一致通过，不一致或缺失时失败。
    #[test]
    fn version_and_identity_contracts() {
        assert!(SupplierProfileCommand::ensure_version(5, 5).is_ok());
        assert!(SupplierProfileCommand::ensure_version(5, 4).is_err());
        assert_eq!(
            SupplierProfileCommand::required_update_version(Some(3), "供应商").unwrap(),
            3
        );
        assert!(SupplierProfileCommand::required_update_version(None, "主体").is_err());
        assert_eq!(
            SupplierProfileCommand::required_create_identity(Some(" SUP-001 "), "供应商编号").unwrap(),
            "SUP-001"
        );
        assert!(SupplierProfileCommand::required_create_identity(Some("   "), "主体编号").is_err());
        assert!(SupplierProfileCommand::required_create_identity(None, "主体编号").is_err());
    }

    fn bare_data(fingerprint: &str) -> SupplierProfileCommandData {
        SupplierProfileCommandData {
            idempotency_key: "key".to_string(),
            operation: "update".to_string(),
            request_fingerprint: fingerprint.to_string(),
            supplier_id: "supplier-1".to_string(),
            supplier_no: "SUP-1".to_string(),
            revision_id: "revision-1".to_string(),
            revision_no: 2,
            supplier_version: 2,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            change_reason: "修订".to_string(),
        }
    }

    fn command_data() -> SupplierProfileCommandData {
        bare_data(ZERO_DIGEST)
    }
}
