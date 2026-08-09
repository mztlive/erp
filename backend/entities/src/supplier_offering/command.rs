//! 供应商供给写命令的幂等结果。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
const OPERATION_MAX_LEN: usize = 64;
const FINGERPRINT_MAX_LEN: usize = 128;

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
    /// 字段为空或超过合同长度时返回错误。
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
}

#[cfg(test)]
mod tests {
    use super::{SupplierOfferingCommand, SupplierOfferingCommandData};

    #[test]
    fn command_normalizes_identity_and_rejects_empty_result() {
        let command = SupplierOfferingCommand::new(
            "command-1",
            SupplierOfferingCommandData {
                idempotency_key: " key-1 ".to_string(),
                operation: " create_offering ".to_string(),
                request_fingerprint: " fingerprint ".to_string(),
                result_json: "{\"offering_id\":\"o1\"}".to_string(),
            },
        )
        .unwrap();
        assert_eq!(command.idempotency_key, "key-1");
        assert_eq!(command.operation, "create_offering");

        let invalid = SupplierOfferingCommandData {
            idempotency_key: "key-2".to_string(),
            operation: "create_offering".to_string(),
            request_fingerprint: "fingerprint".to_string(),
            result_json: " ".to_string(),
        };
        assert!(SupplierOfferingCommand::new("command-2", invalid).is_err());
    }
}
