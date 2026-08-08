//! 供应商资料根级命令去重结果。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;

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
    /// 幂等键、结果身份、请求指纹或变更原因为空，或幂等键过长时返回校验错误。
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
}

#[cfg(test)]
mod tests {
    use crate::common::time::BusinessDate;

    use super::{SupplierProfileCommand, SupplierProfileCommandData};

    #[test]
    fn command_requires_idempotency_key_and_result_identity() {
        let data = SupplierProfileCommandData {
            idempotency_key: " supplier-save-1 ".to_string(),
            operation: "create".to_string(),
            request_fingerprint: "fingerprint-1".to_string(),
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

    fn command_data() -> SupplierProfileCommandData {
        SupplierProfileCommandData {
            idempotency_key: "key".to_string(),
            operation: "update".to_string(),
            request_fingerprint: "fingerprint-1".to_string(),
            supplier_id: "supplier-1".to_string(),
            supplier_no: "SUP-1".to_string(),
            revision_id: "revision-1".to_string(),
            revision_no: 2,
            supplier_version: 2,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            change_reason: "修订".to_string(),
        }
    }
}
