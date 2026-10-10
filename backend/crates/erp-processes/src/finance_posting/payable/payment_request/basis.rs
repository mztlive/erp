//! 实际付款及冲正只能使用原申请不可变的主体、账户和逐采购批准依据。

use erp_finance::entity::payable::{
    PaymentRequestAllocation, PurchasePaymentRequestSubmissionSnapshot, SupplierPayment,
};
use erp_finance::repository::PurchasePaymentRequestSnapshotExt;

use super::*;

/// 沿原申请提交读取不可变依据，不以当前来源或关闭状态替代历史关系。
/// # 参数
/// 原付款、当前申请、真实反向分配及原冲正事务执行器。
/// # 返回
/// 原付款和本次逐采购冲正金额均属于冻结授权时成功。
/// # 错误
/// 缺少申请、快照、主体或逐行关系错配时返回冲突。
pub(super) async fn revalidate(
    db: &Database,
    payment: &SupplierPayment,
    request: &PurchasePaymentRequest,
    allocations: &[PaymentRequestAllocation],
    executor: &mut dyn Executor,
) -> Result<()> {
    let snapshot = db
        .purchase_payment_request_snapshots()
        .find_for_version(&request.base.id, request.submission_version, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("原付款缺少冻结申请依据".into()))?;
    if request.approval_instance_id.as_deref() != Some(snapshot.approval_instance_id.as_str()) {
        return Err(Error::ConflictError("原付款与申请审批实例不一致".into()));
    }
    validate_snapshot(payment, &snapshot)?;
    validate_reversal_allocations(payment, allocations)
}

fn validate_reversal_allocations(
    payment: &SupplierPayment,
    allocations: &[PaymentRequestAllocation],
) -> Result<()> {
    if allocations.is_empty()
        || allocations.iter().any(|allocation| {
            allocation.amount <= Amount::zero()
                || !payment.request_allocations.iter().any(|original| {
                    original.payable_account_id == allocation.payable_account_id
                        && allocation.amount <= original.amount
                })
        })
    {
        return Err(Error::ConflictError("冲正采购或金额不属于原付款分配".into()));
    }
    Ok(())
}

/// 校验原付款与不可变提交的完整身份、主体、账户及逐行批准金额。
/// # 参数
/// 已正式付款和原申请不可变提交。
/// # 返回
/// 所有原授权事实一致时成功，不校验当前来源是否仍可付款。
/// # 错误
/// 任何必填依据缺失、主体、账户或逐行金额错配时返回冲突。
pub(super) fn validate_snapshot(
    payment: &SupplierPayment,
    snapshot: &PurchasePaymentRequestSubmissionSnapshot,
) -> Result<()> {
    let id =
        payment.require_payment_request_basis().map_err(|error| Error::ConflictError(error.to_string()))?;
    let request = &snapshot.request;
    if id != snapshot.request_id
        || snapshot.request_id != request.base.id
        || snapshot.submission_version != request.submission_version
        || snapshot.submission_version == 0
        || request.approval_instance_id.as_deref() != Some(snapshot.approval_instance_id.as_str())
        || request.supplier_id != payment.supplier_id.as_ref()
        || payment.payer_party_id.as_deref() != Some(request.payer_party_id.as_str())
        || payment.payer_party_name.as_deref() != Some(request.payer_party_name.as_str())
        || payment.payee_bank_account_id.as_ref().map(AsRef::as_ref)
            != request.payee_account.as_ref().map(|bank| bank.account_id.as_str())
        || request.payee_account.is_none()
        || payment.request_allocations.iter().any(|allocation| {
            !request.lines.iter().any(|line| {
                line.payable_account_id == allocation.payable_account_id
                    && allocation.amount <= line.requested_amount
            })
        })
    {
        return Err(Error::ConflictError("原付款与冻结申请主体、账户或逐采购金额不一致".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{FileAssetId, PartyBankAccountId, SupplierAccountId, SupplierPaymentId};
    use erp_finance::entity::payable::{
        PaymentRequestAccountSnapshot, PaymentRequestLineData, PaymentRequestSalesSource,
        PaymentRequestSourceSnapshot, PaymentRequestSubmission, PurchasePaymentRequestData,
        PurchasePaymentRequestLine, SupplierPaymentData,
    };
    use serde_json::json;

    use super::*;

    fn amount(value: &str) -> Amount {
        value.parse().unwrap()
    }

    fn allocation(id: &str, value: &str) -> PaymentRequestAllocation {
        PaymentRequestAllocation { payable_account_id: id.into(), amount: amount(value) }
    }

    fn fixture() -> (SupplierPayment, PurchasePaymentRequestSubmissionSnapshot) {
        let mut request = PurchasePaymentRequest::new(
            "request-1".into(),
            PurchasePaymentRequestData {
                supplier_id: "supplier-1".into(),
                payee_bank_account_id: "bank-1".into(),
                lines: vec![PaymentRequestLineData {
                    payable_account_id: "account-1".into(),
                    amount: amount("20"),
                }],
                desired_payment_date: BusinessDate::from_ymd(2026, 10, 10).unwrap(),
                reason: "逐采购复核付款".into(),
                attachment_ids: Vec::new(),
            },
            "buyer-1",
        )
        .unwrap();
        request
            .submit(PaymentRequestSubmission {
                supplier_id: "supplier-1".into(),
                payer_party_id: "company-1".into(),
                payer_party_name: "我方公司".into(),
                org_unit_id: Some("procurement".into()),
                payee_account: PaymentRequestAccountSnapshot {
                    account_id: "bank-1".into(),
                    version: 1,
                    supplier_party_id: "supplier-party".into(),
                    account_name: "供应商".into(),
                    bank_name: "开户行".into(),
                    masked_account_number: "****1234".into(),
                },
                lines: vec![PurchasePaymentRequestLine {
                    payable_account_id: "account-1".into(),
                    purchase_order_id: "purchase-1".into(),
                    purchase_revision_id: "purchase-v1".into(),
                    requested_amount: amount("20"),
                    paid_amount: Amount::zero(),
                    closed_amount: Amount::zero(),
                    gross_total: amount("20"),
                    settled_total: Amount::zero(),
                    open_total: amount("20"),
                    other_reserved_total: Amount::zero(),
                    payment_terms: json!({}),
                    source_snapshot: PaymentRequestSourceSnapshot {
                        purchase_preview: json!({"revision_id":"purchase-v1"}),
                        sales_sources: vec![PaymentRequestSalesSource {
                            sales_order_id: "sales-1".into(),
                            sales_revision_id: "sales-v1".into(),
                            customer_id: "customer-1".into(),
                            payer_party_id: "company-1".into(),
                            payer_party_name: "我方公司".into(),
                            preview: json!({"revision_id":"sales-v1"}),
                        }],
                        allowed_attachment_ids: Vec::new(),
                    },
                }],
            })
            .unwrap();
        request.approval_instance_id = Some("instance-1".into());
        let snapshot = PurchasePaymentRequestSubmissionSnapshot::freeze(&request).unwrap();
        let mut payment = SupplierPayment::new(
            SupplierPaymentId::new("payment-1"),
            SupplierPaymentData {
                payment_no: "FK-1".into(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                payee_bank_account_id: PartyBankAccountId::new("bank-1"),
                paid_at: Instant::from_unix_secs(1),
                amount: amount("10"),
                bank_reference: None,
                bank_receipt_asset_id: FileAssetId::new("receipt-1"),
            },
            "cashier-1",
        )
        .unwrap();
        payment.payment_request_id = Some("request-1".into());
        payment.payer_party_id = Some("company-1".into());
        payment.payer_party_name = Some("我方公司".into());
        payment.request_allocations = vec![allocation("account-1", "10")];
        (payment, snapshot)
    }

    #[test]
    fn original_partial_payment_and_closed_request_keep_the_same_immutable_basis() {
        let (payment, snapshot) = fixture();
        let mut current = snapshot.request.clone();
        current.approve().unwrap();
        current.record_payment(&payment.request_allocations).unwrap();
        current.close("关闭未付金额").unwrap();
        assert_eq!(current.status, PurchasePaymentRequestStatus::Closed);
        assert!(validate_snapshot(&payment, &snapshot).is_ok());
        assert!(validate_reversal_allocations(&payment, &[allocation("account-1", "5")]).is_ok());
        assert_eq!(snapshot.request.lines[0].paid_amount, Amount::zero());
    }

    #[test]
    fn missing_original_request_or_frozen_identity_cannot_authorize_reversal() {
        let (payment, snapshot) = fixture();
        let mut missing = payment.clone();
        missing.payment_request_id = None;
        assert!(matches!(validate_snapshot(&missing, &snapshot), Err(Error::ConflictError(_))));
        let mut wrong_request = payment.clone();
        wrong_request.payment_request_id = Some("request-2".into());
        assert!(validate_snapshot(&wrong_request, &snapshot).is_err());
        for tamper in ["request", "version", "instance"] {
            let mut bad = snapshot.clone();
            match tamper {
                "request" => bad.request_id = "request-2".into(),
                "version" => bad.submission_version = 2,
                _ => bad.approval_instance_id = "instance-2".into(),
            }
            assert!(validate_snapshot(&payment, &bad).is_err());
        }
    }

    #[test]
    fn changed_payer_supplier_bank_or_purchase_allocation_is_rejected() {
        let (payment, snapshot) = fixture();
        let mut variants = Vec::new();
        let mut bad = payment.clone();
        bad.payer_party_id = Some("company-2".into());
        variants.push(bad);
        let mut bad = payment.clone();
        bad.payer_party_name = Some("另一公司".into());
        variants.push(bad);
        let mut bad = payment.clone();
        bad.supplier_id = SupplierAccountId::new("supplier-2");
        variants.push(bad);
        let mut bad = payment.clone();
        bad.payee_bank_account_id = Some(PartyBankAccountId::new("bank-2"));
        variants.push(bad);
        let mut bad = payment.clone();
        bad.request_allocations = vec![allocation("unrelated", "10")];
        variants.push(bad);
        let mut bad = payment.clone();
        bad.amount = amount("21");
        bad.request_allocations = vec![allocation("account-1", "21")];
        variants.push(bad);
        for bad in variants {
            assert!(validate_snapshot(&bad, &snapshot).is_err());
        }
        for allocations in [
            Vec::new(),
            vec![allocation("account-1", "0")],
            vec![allocation("unrelated", "5")],
            vec![allocation("account-1", "11")],
        ] {
            assert!(validate_reversal_allocations(&payment, &allocations).is_err());
        }
    }
}
