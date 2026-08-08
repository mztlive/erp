//! 供应商商品库写命令的幂等结果。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
const OPERATION_MAX_LEN: usize = 64;
const FINGERPRINT_MAX_LEN: usize = 128;

/// 已成功供应商商品库写命令的持久化数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogCommandData {
    /// 客户端生成的幂等键。
    pub idempotency_key: String,
    /// 稳定操作名。
    pub operation: String,
    /// 包含路径目标与请求体的 SHA-256 指纹。
    pub request_fingerprint: String,
    /// 序列化后的稳定响应结果。
    pub result_json: String,
}

/// 供应商商品库命令去重记录；必须与业务写入处于同一事务。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierCatalogCommand {
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

impl SupplierCatalogCommand {
    /// 创建已成功命令的去重记录。
    ///
    /// # Errors
    /// 幂等键、操作名、指纹或结果为空，或文本超过合同长度时返回错误。
    pub fn new(id: impl Into<String>, data: SupplierCatalogCommandData) -> Result<Self> {
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
    use super::{SupplierCatalogCommand, SupplierCatalogCommandData};

    #[test]
    fn command_normalizes_identity_and_rejects_empty_result() {
        let command = SupplierCatalogCommand::new(
            "command-1",
            SupplierCatalogCommandData {
                idempotency_key: " key-1 ".to_string(),
                operation: " create_product ".to_string(),
                request_fingerprint: " fingerprint ".to_string(),
                result_json: "{\"product_id\":\"p1\"}".to_string(),
            },
        )
        .unwrap();
        assert_eq!(command.idempotency_key, "key-1");
        assert_eq!(command.operation, "create_product");

        let invalid = SupplierCatalogCommandData {
            idempotency_key: "key-2".to_string(),
            operation: "create_product".to_string(),
            request_fingerprint: "fingerprint".to_string(),
            result_json: " ".to_string(),
        };
        assert!(SupplierCatalogCommand::new("command-2", invalid).is_err());
    }
}
