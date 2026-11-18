//! 供应商资料根级命令去重结果。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::BusinessDate;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

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
    /// 返回已校验的去重记录；请求指纹按传入原样保存。
    /// 幂等键、结果身份、请求指纹或变更原因为空，或幂等键过长、指纹格式非法时返回校验错误。
    /// 请求指纹接受 `sha256-v1:<64hex>` 或历史裸 `64hex`；存储按传入原样保留，比较时按摘要兼容。
    /// `data`：幂等键、操作、指纹与结果身份。
    pub fn new(id: impl Into<String>, data: SupplierProfileCommandData) -> Result<Self> {
        let idempotency_key = normalize_required_text(
            data.idempotency_key,
            "缺少本次操作的提交编号，请重新发起操作",
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
        ProfileFingerprint::parse(&data.request_fingerprint)?;

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
    /// 操作、目标与指纹均一致时返回 `Ok(())`。
    /// 操作、指纹或目标任一不一致时返回校验错误。
    /// 纯内存校验，不触及 MongoDB 或时钟；兼容版本前缀与裸摘要，摘要大小写不敏感。
    /// `supplier_id`：仅 `update` 时校验目标供应商 ID，`create` 传入 `None`。
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

    /// 核对乐观锁版本。
    /// 版本一致时返回 `Ok(())`。
    /// 版本不一致时返回校验错误。
    /// 纯内存比较，不触及外部状态。
    pub fn ensure_version(actual: u64, expected: u64) -> Result<()> {
        if actual != expected {
            return Err(Error::from("数据已被其他请求修改，请刷新后重试"));
        }
        Ok(())
    }

    /// 读取更新场景必填版本号。
    /// 存在时返回版本号。
    /// `value` 为 `None` 时返回校验错误。
    /// 仅做存在性校验，不触及持久化。
    /// `value`：可空版本输入。
    pub fn required_update_version(value: Option<u64>, object: &str) -> Result<u64> {
        value.ok_or_else(|| Error::from(format!("修订供应商时{object}版本不能为空")))
    }

    /// 校验创建场景必填的稳定业务编号。
    /// 去首尾空白后非空时返回规范化编号。
    /// 输入为空或全空白时返回校验错误。
    /// 仅做空白与存在性校验，不触及唯一性查询。
    /// `value`：可空输入。
    pub fn required_create_identity(value: Option<&str>, field: &str) -> Result<String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| Error::from(format!("创建供应商时{field}不能为空")))
    }
}

/// 已解析的资料请求指纹；只保存合法摘要视图，不改写持久化原值。
struct ProfileFingerprint<'a>(&'a str);

impl<'a> ProfileFingerprint<'a> {
    // 在创建或历史回执读取边界解析一次，内部比较直接使用合法摘要。
    fn parse(value: &'a str) -> Result<Self> {
        let digest = value.strip_prefix(FINGERPRINT_V1_PREFIX).unwrap_or(value);
        if digest.len() != FINGERPRINT_HEX_LEN
            || !digest.bytes().all(|b| b.is_ascii_hexdigit())
            || value.trim() != value
        {
            return Err(Error::from("供应商资料请求指纹格式无效"));
        }
        Ok(Self(digest))
    }
}

// 持久化记录和当前请求分别解析后，按大小写不敏感的摘要比较。
fn fingerprints_match(a: &str, b: &str) -> bool {
    let Ok(stored) = ProfileFingerprint::parse(a) else { return false };
    let Ok(requested) = ProfileFingerprint::parse(b) else { return false };
    stored.0.eq_ignore_ascii_case(requested.0)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;

    use super::{FINGERPRINT_V1_PREFIX, SupplierProfileCommand, SupplierProfileCommandData};

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

        let invalid = SupplierProfileCommandData { idempotency_key: " ".to_string(), ..command_data() };
        assert!(SupplierProfileCommand::new("command-2", invalid).is_err());
    }

    // 验证同键同载荷可重放，操作、目标或指纹任一不同必须冲突。
    #[test]
    fn replay_is_bound_to_operation_target_and_fingerprint() {
        let command = SupplierProfileCommand::new("command-1", command_data()).unwrap();
        assert!(command.ensure_replayable("update", Some("supplier-1"), ZERO_DIGEST).is_ok());
        assert!(command.ensure_replayable("create", Some("supplier-1"), ZERO_DIGEST).is_err());
        assert!(command.ensure_replayable("update", Some("supplier-2"), ZERO_DIGEST).is_err());
        assert!(command.ensure_replayable("update", Some("supplier-1"), ONE_DIGEST).is_err());

        // create 场景的 supplier_id 为 None 时不校验目标
        let create = SupplierProfileCommand::new(
            "command-create",
            SupplierProfileCommandData { operation: "create".to_string(), ..command_data() },
        )
        .unwrap();
        assert!(create.ensure_replayable("create", None, ZERO_DIGEST).is_ok());
    }

    // 验证前带 `sha256-v1:` 与裸摘要按摘要兼容。
    #[test]
    fn fingerprint_versioned_and_legacy_are_compatible() {
        let bare = SupplierProfileCommand::new("cmd-bare", bare_data(ZERO_DIGEST)).unwrap();
        let versioned = SupplierProfileCommand::new(
            "cmd-versioned",
            bare_data(&format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}")),
        )
        .unwrap();

        assert!(bare.ensure_replayable("update", Some("supplier-1"), ZERO_DIGEST).is_ok());
        assert!(
            bare.ensure_replayable(
                "update",
                Some("supplier-1"),
                &format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}")
            )
            .is_ok()
        );
        assert!(versioned.ensure_replayable("update", Some("supplier-1"), ZERO_DIGEST).is_ok());
        assert!(versioned.ensure_replayable("update", Some("supplier-1"), ONE_DIGEST).is_err());

        let doc = serde_json::to_value(&bare).unwrap();
        let roundtrip: SupplierProfileCommand = serde_json::from_value(doc).unwrap();
        assert_eq!(roundtrip.request_fingerprint, ZERO_DIGEST);
        let doc2 = serde_json::to_value(&versioned).unwrap();
        let roundtrip2: SupplierProfileCommand = serde_json::from_value(doc2).unwrap();
        assert_eq!(roundtrip2.request_fingerprint, format!("{FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}"));
    }

    // 验证非法长度或非十六进制拒绝。
    #[test]
    fn fingerprint_format_rejects_invalid() {
        let bad =
            SupplierProfileCommandData { request_fingerprint: "not-a-hex".to_string(), ..command_data() };
        assert!(SupplierProfileCommand::new("bad-1", bad).is_err());
        let padded =
            SupplierProfileCommandData { request_fingerprint: format!(" {ZERO_DIGEST}"), ..command_data() };
        assert!(SupplierProfileCommand::new("bad-2", padded).is_err());
    }

    #[test]
    fn replay_parses_historical_fingerprints_without_rewriting_stored_values() {
        let digest = "a".repeat(64);
        let stored = format!("{FINGERPRINT_V1_PREFIX}{}", digest.to_ascii_uppercase());
        let command = SupplierProfileCommand::new("command", bare_data(&stored)).unwrap();
        command.ensure_replayable("update", Some("supplier-1"), &digest).unwrap();
        assert_eq!(command.request_fingerprint, stored);

        // 数据库反序列化可绕过构造；历史坏值与缺失值仍须在重放边界拒绝。
        for invalid in [
            String::new(),
            "g".repeat(64),
            format!(" {digest}"),
            format!("{digest} "),
            format!("SHA256-V1:{digest}"),
        ] {
            assert!(SupplierProfileCommand::new("command", bare_data(&invalid)).is_err());
            let mut document = serde_json::to_value(&command).unwrap();
            document["request_fingerprint"] = serde_json::json!(invalid);
            let historical: SupplierProfileCommand = serde_json::from_value(document).unwrap();
            assert_eq!(
                historical
                    .ensure_replayable("update", Some("other-supplier"), &invalid)
                    .unwrap_err()
                    .to_string(),
                "幂等键已用于不同的供应商资料请求"
            );
            assert!(command.ensure_replayable("update", Some("supplier-1"), &invalid).is_err());
        }

        let mut document = serde_json::to_value(&command).unwrap();
        document.as_object_mut().unwrap().remove("request_fingerprint");
        let historical: SupplierProfileCommand = serde_json::from_value(document).unwrap();
        assert!(historical.ensure_replayable("update", Some("supplier-1"), &digest).is_err());
    }

    // 验证版本一致通过，不一致或缺失时失败。
    #[test]
    fn version_and_identity_contracts() {
        assert!(SupplierProfileCommand::ensure_version(5, 5).is_ok());
        assert!(SupplierProfileCommand::ensure_version(5, 4).is_err());
        assert_eq!(SupplierProfileCommand::required_update_version(Some(3), "供应商").unwrap(), 3);
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
