//! 客户回款原单草稿编辑合同。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::receivable::CustomerReceiptUpdate;

/// 修改原客户回款草稿的到账金额、时间或银行流水引用。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateCustomerReceiptRequest {
    /// 当前原单的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 到账时间；省略保持原值。
    pub received_at: Option<Instant>,
    /// 到账金额；省略保持原值。
    pub amount: Option<Amount>,
    /// 银行流水引用；省略保持原值，空字符串清除。
    pub bank_reference: Option<String>,
}

impl From<UpdateCustomerReceiptRequest> for CustomerReceiptUpdate {
    /// 只映射实体允许修改的字段，原单号、来源及创建人不进入更新数据。
    fn from(value: UpdateCustomerReceiptRequest) -> Self {
        Self { received_at: value.received_at, amount: value.amount, bank_reference: value.bank_reference }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use validator::Validate;

    use super::UpdateCustomerReceiptRequest;
    use crate::entity::receivable::CustomerReceiptUpdate;

    /// 草稿编辑只接受原可变字段与非零版本。
    #[test]
    fn request_maps_mutable_fields_and_rejects_identity_changes() {
        let valid =
            json!({"version": 3, "amount": "50", "received_at": 1700000000, "bank_reference": "BANK-1"});
        let request: UpdateCustomerReceiptRequest = serde_json::from_value(valid.clone()).unwrap();
        request.validate().unwrap();
        let update = CustomerReceiptUpdate::from(request);
        assert_eq!(update.amount.unwrap().to_string(), "50");
        assert_eq!(update.received_at.unwrap().unix_secs(), 1700000000);
        assert_eq!(update.bank_reference.as_deref(), Some("BANK-1"));
        for key in ["receipt_no", "counterparty_party_id", "customer_id", "created_by"] {
            let mut forged = valid.clone();
            forged[key] = json!("forged");
            assert!(serde_json::from_value::<UpdateCustomerReceiptRequest>(forged).is_err());
        }
        let invalid: UpdateCustomerReceiptRequest = serde_json::from_value(json!({"version": 0})).unwrap();
        assert!(invalid.validate().is_err());
    }
}
