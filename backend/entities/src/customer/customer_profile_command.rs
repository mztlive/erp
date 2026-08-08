//! `customer_profile_command`：客户资料根级保存命令的幂等结果事实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

/// 客户端幂等键最大长度。
const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;

/// 已成功客户资料命令的数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerProfileCommandData {
    /// 客户端幂等键。
    pub idempotency_key: String,
    /// 稳定操作名：`create` 或 `update`。
    pub operation: String,
    /// 发起命令的账号 ID。
    pub initiated_by: String,
    /// 规范化请求的 SHA-256 指纹。
    pub request_fingerprint: String,
    /// 客户角色 ID。
    pub customer_id: String,
    /// 客户编号。
    pub customer_no: String,
    /// Party ID。
    pub party_id: String,
    /// 当前 Party 修订 ID。
    pub revision_id: String,
    /// 当前 Party 修订号。
    pub revision_no: u32,
    /// 保存后的客户乐观锁版本。
    pub customer_version: u64,
    /// 保存后的 Party 乐观锁版本。
    pub party_version: u64,
    /// 从属事实生效日期。
    pub effective_from: BusinessDate,
    /// 原始变更原因。
    pub change_reason: String,
}

/// 已成功客户资料根级命令。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct CustomerProfileCommand {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 客户端幂等键。
    pub idempotency_key: String,
    /// 稳定操作名。
    pub operation: String,
    /// 发起命令的账号 ID。
    pub initiated_by: String,
    /// 规范化请求指纹。
    pub request_fingerprint: String,
    /// 客户角色 ID。
    pub customer_id: String,
    /// 客户编号。
    pub customer_no: String,
    /// Party ID。
    pub party_id: String,
    /// 当前 Party 修订 ID。
    pub revision_id: String,
    /// 当前 Party 修订号。
    pub revision_no: u32,
    /// 保存后的客户乐观锁版本。
    pub customer_version: u64,
    /// 保存后的 Party 乐观锁版本。
    pub party_version: u64,
    /// 从属事实生效日期。
    pub effective_from: BusinessDate,
    /// 原始变更原因。
    pub change_reason: String,
}

impl CustomerProfileCommand {
    /// 创建客户资料命令的稳定成功结果。
    ///
    /// # Errors
    /// 幂等键、操作、请求指纹、结果身份或变更原因为空时返回校验错误。
    pub fn new(id: impl Into<String>, data: CustomerProfileCommandData) -> Result<Self> {
        let idempotency_key = normalize_required_text(
            data.idempotency_key,
            "幂等键不能为空",
            IDEMPOTENCY_KEY_MAX_LEN,
            "幂等键过长",
        )?;
        let required = [
            data.operation.as_str(),
            data.initiated_by.as_str(),
            data.request_fingerprint.as_str(),
            data.customer_id.as_str(),
            data.customer_no.as_str(),
            data.party_id.as_str(),
            data.revision_id.as_str(),
            data.change_reason.as_str(),
        ];
        if required.iter().any(|value| value.trim().is_empty()) {
            return Err(Error::from("客户资料命令结果身份、请求指纹或变更原因不能为空"));
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            idempotency_key,
            operation: data.operation,
            initiated_by: data.initiated_by,
            request_fingerprint: data.request_fingerprint,
            customer_id: data.customer_id,
            customer_no: data.customer_no,
            party_id: data.party_id,
            revision_id: data.revision_id,
            revision_no: data.revision_no,
            customer_version: data.customer_version,
            party_version: data.party_version,
            effective_from: data.effective_from,
            change_reason: data.change_reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::common::time::BusinessDate;

    use super::{CustomerProfileCommand, CustomerProfileCommandData};

    /// happy path：幂等键规范化并保留稳定结果身份。
    #[test]
    fn command_normalizes_key_and_keeps_result_identity() {
        let command = CustomerProfileCommand::new("command-1", command_data()).unwrap();
        assert_eq!(command.idempotency_key, "customer-save-1");
        assert_eq!(command.customer_id, "customer-1");
        assert_eq!(command.revision_no, 2);
    }

    /// 失败路径：空幂等键或空请求指纹被拒绝。
    #[test]
    fn command_rejects_missing_idempotency_identity() {
        let blank_key = CustomerProfileCommandData {
            idempotency_key: " ".to_string(),
            ..command_data()
        };
        assert!(CustomerProfileCommand::new("command-2", blank_key).is_err());

        let blank_fingerprint = CustomerProfileCommandData {
            request_fingerprint: " ".to_string(),
            ..command_data()
        };
        assert!(CustomerProfileCommand::new("command-3", blank_fingerprint).is_err());
    }

    /// 构造最小合法命令数据。
    fn command_data() -> CustomerProfileCommandData {
        CustomerProfileCommandData {
            idempotency_key: " customer-save-1 ".to_string(),
            operation: "update".to_string(),
            initiated_by: "admin-1".to_string(),
            request_fingerprint: "fingerprint-1".to_string(),
            customer_id: "customer-1".to_string(),
            customer_no: "KH-1".to_string(),
            party_id: "party-1".to_string(),
            revision_id: "revision-2".to_string(),
            revision_no: 2,
            customer_version: 2,
            party_version: 2,
            effective_from: BusinessDate::from_ymd(2026, 8, 8).unwrap(),
            change_reason: "资料修订".to_string(),
        }
    }
}
