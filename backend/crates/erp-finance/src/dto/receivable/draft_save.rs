//! 可恢复的客户回款草稿保存命令。
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{CreateCustomerReceiptRequest, ReceiptAllocationLineRequest};
use crate::{Error, Result};

/// 保存完整到账信息与拟核销分配；保存不启动审批。
#[derive(Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SaveCustomerReceiptDraftRequest {
    /// 已有原单；新建时为空。
    pub receipt_id: Option<String>,
    /// 已有原单版本；新建时为空。
    pub expected_version: Option<u64>,
    /// 完整到账字段；已有原单的身份字段不得改变。
    pub receipt: CreateCustomerReceiptRequest,
    /// 拟核销分配，可为空；已提交过的原单仍须保留原来源。
    pub allocations: Vec<ReceiptAllocationLineRequest>,
    /// 保存操作号；结果未知时须使用原号和原内容重试。
    #[validate(length(min = 1, max = 128))]
    pub idempotency_key: String,
}
impl SaveCustomerReceiptDraftRequest {
    /// 校验新建或原单编辑的互斥身份。
    /// # 参数
    /// 无。
    /// # 返回
    /// 合法身份返回空结果。
    /// # 错误
    /// 缺失原单版本、空主键或新建携带版本时拒绝。
    pub fn validate_identity(&self) -> Result<()> {
        match (&self.receipt_id, self.expected_version) {
            (None, None) => Ok(()),
            (Some(id), Some(version)) if !id.trim().is_empty() && version > 0 => Ok(()),
            _ => Err(Error::ValidationError("已有草稿必须提供原单与当前版本".into())),
        }
    }
}
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    #[test]
    fn draft_identity_requires_matching_id_and_version() {
        let mut value = json!({"receipt": {"receipt_no":"R1", "counterparty_party_id":"p", "received_at":1, "amount":"10"}, "allocations":[], "idempotency_key":"save-1"});
        let parse = |v| serde_json::from_value::<SaveCustomerReceiptDraftRequest>(v).unwrap();
        parse(value.clone()).validate_identity().unwrap();
        value["receipt_id"] = json!("r1");
        assert!(parse(value.clone()).validate_identity().is_err());
        value["expected_version"] = json!(0);
        assert!(parse(value.clone()).validate_identity().is_err());
        value["expected_version"] = json!(1);
        parse(value).validate_identity().unwrap();
    }
}
