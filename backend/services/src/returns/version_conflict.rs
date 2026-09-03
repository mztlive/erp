//! 乐观锁版本冲突到稳定 409 的 Service 映射（SALES-E18）。
//!
//! 实体 `matches_version` 拥有比较规则；本模块只把 `false` 映射为 ConflictError。

use crate::errors::{Error, Result};

/// 调用方读取版本与当前实体不一致时的稳定冲突文案。
const STALE_VERSION_MESSAGE: &str = "数据已被其他请求修改，请刷新后重试";

/// 将实体版本匹配结果映射为稳定 409 语义。
///
/// # 参数
/// * `matched` - `Entity::matches_version(expected)` 的结果
///
/// # 返回
/// 匹配时返回 `Ok(())`。
///
/// # 错误
/// 不匹配时返回 `ConflictError`（HTTP 409）。
///
/// # 约束
/// 不比较版本号；调用方必须先调用实体 `matches_version`。
pub fn conflict_if_stale_version(matched: bool) -> Result<()> {
    if matched {
        return Ok(());
    }
    Err(Error::ConflictError(STALE_VERSION_MESSAGE.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{conflict_if_stale_version, STALE_VERSION_MESSAGE};
    use crate::errors::Error;
    use entities::common::time::Instant;
    use entities::ids::{
        CustomerAccountId, CustomerReceiptId, CustomerRefundId, PaymentReversalId, ReceiptReversalId,
        SupplierAccountId, SupplierPaymentId, SupplierRefundId,
    };
    use entities::money::Amount;
    use entities::returns::{
        CustomerRefund, CustomerRefundData, PaymentReversal, PaymentReversalData, ReceiptReversal,
        ReceiptReversalData, SupplierRefund, SupplierRefundData,
    };
    use std::str::FromStr;

    #[test]
    fn stale_version_returns_conflict_with_stable_retry_message() {
        assert!(conflict_if_stale_version(true).is_ok());
        match conflict_if_stale_version(false) {
            Err(Error::ConflictError(message)) => assert_eq!(message, STALE_VERSION_MESSAGE),
            other => panic!("必须映射为 ConflictError，得到 {other:?}"),
        }
    }

    #[test]
    fn refund_and_reversal_entities_map_stale_version_to_conflict() {
        let refund = CustomerRefund::new(
            CustomerRefundId::new("crf-1"),
            CustomerRefundData {
                refund_no: "RF-1".into(),
                sales_return_case_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                original_receipt_id: Some(CustomerReceiptId::new("cr-1")),
                original_receivable_entry_id: None,
                reason_code: None,
                reason_text: "质量退款".into(),
                amount: Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(1),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造");
        assert!(conflict_if_stale_version(refund.matches_version(refund.base.version)).is_ok());
        assert!(matches!(
            conflict_if_stale_version(refund.matches_version(0)),
            Err(Error::ConflictError(message)) if message == STALE_VERSION_MESSAGE
        ));

        let supplier = SupplierRefund::new(
            SupplierRefundId::new("srf-1"),
            SupplierRefundData {
                refund_no: "SRF-1".into(),
                purchase_return_order_id: None,
                supplier_id: SupplierAccountId::new("sup-1"),
                original_payment_id: Some(SupplierPaymentId::new("sp-1")),
                original_payable_entry_id: None,
                reason_code: None,
                reason_text: "错付款退回".into(),
                amount: Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(1),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造");
        assert!(matches!(
            conflict_if_stale_version(supplier.matches_version(0)),
            Err(Error::ConflictError(_))
        ));

        let receipt = ReceiptReversal::new(
            ReceiptReversalId::new("rr-1"),
            ReceiptReversalData {
                reversal_no: "RR-1".into(),
                original_customer_receipt_id: CustomerReceiptId::new("cr-1"),
                reason_code: None,
                reason_text: "错记回款冲正".into(),
                amount: Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(1),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造");
        assert!(matches!(
            conflict_if_stale_version(receipt.matches_version(0)),
            Err(Error::ConflictError(_))
        ));

        let payment = PaymentReversal::new(
            PaymentReversalId::new("prr-1"),
            PaymentReversalData {
                reversal_no: "PRR-1".into(),
                original_supplier_payment_id: SupplierPaymentId::new("sp-1"),
                reason_code: None,
                reason_text: "错付款冲正".into(),
                amount: Amount::from_str("100").expect("金额合法"),
                handled_by: "handler-1".into(),
                reviewed_by: "reviewer-1".into(),
                occurred_at: Instant::from_unix_secs(1),
                evidence_attachment_id: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造");
        assert!(matches!(
            conflict_if_stale_version(payment.matches_version(0)),
            Err(Error::ConflictError(_))
        ));
    }
}
