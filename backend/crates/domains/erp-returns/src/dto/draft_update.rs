//! 退款与冲正原单草稿的可编辑字段合同。

use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::returns::{
    CustomerRefundUpdate, PaymentReversalUpdate, ReceiptReversalUpdate, SupplierRefundUpdate,
};

/// 编辑客户/供应商退款、回款/付款冲正的原单草稿。
///
/// 单号、原资金来源、创建人、经办人、复核人和发生时间均不允许变更。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateFinancialReturnRequest {
    /// 当前原单的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 退款或冲正金额；省略保持原值。
    pub amount: Option<Amount>,
    /// 原因代码；省略保持原值，空字符串清除。
    pub reason_code: Option<String>,
    /// 原因说明；省略保持原值，提供时必须非空。
    pub reason_text: Option<String>,
}

impl From<UpdateFinancialReturnRequest> for CustomerRefundUpdate {
    /// 将请求的可编辑字段映射为客户退款更新数据。
    fn from(value: UpdateFinancialReturnRequest) -> Self {
        Self {
            amount: value.amount,
            reason_code: value.reason_code,
            reason_text: value.reason_text,
            evidence_attachment_id: None,
        }
    }
}

impl From<UpdateFinancialReturnRequest> for SupplierRefundUpdate {
    /// 将请求的可编辑字段映射为供应商退款更新数据。
    fn from(value: UpdateFinancialReturnRequest) -> Self {
        Self {
            amount: value.amount,
            reason_code: value.reason_code,
            reason_text: value.reason_text,
            evidence_attachment_id: None,
        }
    }
}

impl From<UpdateFinancialReturnRequest> for ReceiptReversalUpdate {
    /// 将请求的可编辑字段映射为回款冲正更新数据。
    fn from(value: UpdateFinancialReturnRequest) -> Self {
        Self {
            amount: value.amount,
            reason_code: value.reason_code,
            reason_text: value.reason_text,
            evidence_attachment_id: None,
        }
    }
}

impl From<UpdateFinancialReturnRequest> for PaymentReversalUpdate {
    /// 将请求的可编辑字段映射为付款冲正更新数据。
    fn from(value: UpdateFinancialReturnRequest) -> Self {
        Self {
            amount: value.amount,
            reason_code: value.reason_code,
            reason_text: value.reason_text,
            evidence_attachment_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use validator::Validate;

    use super::UpdateFinancialReturnRequest;
    use crate::entity::returns::CustomerRefundUpdate;

    /// 编辑合同只接受现有可变字段与非零版本。
    #[test]
    fn edit_request_rejects_fixed_fields_and_maps_mutable_fields() {
        let valid = json!({"version": 2, "amount": "20", "reason_text": "更正退款金额"});
        let request: UpdateFinancialReturnRequest = serde_json::from_value(valid.clone()).unwrap();
        request.validate().unwrap();
        let update = CustomerRefundUpdate::from(request);
        assert_eq!(update.amount.unwrap().to_string(), "20");
        assert_eq!(update.reason_text.as_deref(), Some("更正退款金额"));
        for key in [
            "refund_no",
            "original_receipt_id",
            "handled_by",
            "reviewed_by",
            "occurred_at",
            "evidence_attachment_id",
        ] {
            let mut forged = valid.clone();
            forged[key] = json!("forged");
            assert!(serde_json::from_value::<UpdateFinancialReturnRequest>(forged).is_err());
        }
        let invalid: UpdateFinancialReturnRequest = serde_json::from_value(json!({"version": 0})).unwrap();
        assert!(invalid.validate().is_err());
    }
}
