use erp_finance::entity::payable::{
    PaymentRequestLineData, PaymentRequestSourceSnapshot, PurchasePaymentRequestData,
    PurchasePaymentRequestLine,
};
use serde_json::json;

use super::*;

fn request() -> PurchasePaymentRequest {
    let mut request = PurchasePaymentRequest::new(
        "request".into(),
        PurchasePaymentRequestData {
            supplier_id: "supplier".into(),
            payee_bank_account_id: "bank".into(),
            desired_payment_date: BusinessDate::from_ymd(2026, 10, 12).unwrap(),
            reason: "采购付款".into(),
            attachment_ids: vec![],
            lines: vec![PaymentRequestLineData {
                payable_account_id: "account".into(),
                amount: "60".parse().unwrap(),
            }],
        },
        "buyer",
    )
    .unwrap();
    request.lines = vec![line("purchase", "account", "60"), line("other", "other-account", "900")];
    request.total_amount = "960".parse().unwrap();
    request.status = PurchasePaymentRequestStatus::Approved;
    request
}

fn line(purchase: &str, account: &str, amount: &str) -> PurchasePaymentRequestLine {
    PurchasePaymentRequestLine {
        purchase_order_id: purchase.into(),
        payable_account_id: account.into(),
        purchase_revision_id: "revision".into(),
        requested_amount: amount.parse().unwrap(),
        paid_amount: Amount::zero(),
        closed_amount: Amount::zero(),
        gross_total: "1000".parse().unwrap(),
        settled_total: Amount::zero(),
        open_total: "1000".parse().unwrap(),
        other_reserved_total: Amount::zero(),
        payment_terms: json!({}),
        source_snapshot: PaymentRequestSourceSnapshot {
            purchase_preview: json!({"private": "other source data"}),
            sales_sources: vec![],
            allowed_attachment_ids: vec!["private-file".into()],
        },
    }
}

#[test]
fn combined_request_exposes_only_matching_purchase_and_account() {
    let request = request();
    let purchase = PurchaseOrderId::new("purchase");
    let view = FulfillmentRequestView::for_purchase(&request, &purchase, "account").unwrap();
    assert_eq!(view.requested_amount, "60".parse().unwrap());
    assert_eq!(view.paid_amount, Amount::zero());
    assert_eq!(view.remaining_amount, "60".parse().unwrap());
    assert!(FulfillmentRequestView::for_purchase(&request, &purchase, "other-account").is_none());
    assert!(
        FulfillmentRequestView::for_purchase(&request, &PurchaseOrderId::new("missing"), "account").is_none()
    );
    let response = serde_json::to_value(view).unwrap();
    assert!(response.get("total_amount").is_none());
    assert!(response.get("source_snapshot").is_none());
    assert!(response.get("payee_account").is_none());
}

#[test]
fn approval_never_implies_payment_and_closed_amount_is_separate() {
    let mut request = request();
    let purchase = PurchaseOrderId::new("purchase");
    for status in [
        PurchasePaymentRequestStatus::InApproval,
        PurchasePaymentRequestStatus::Approved,
        PurchasePaymentRequestStatus::Rejected,
        PurchasePaymentRequestStatus::Withdrawn,
    ] {
        request.status = status;
        let view = FulfillmentRequestView::for_purchase(&request, &purchase, "account").unwrap();
        assert_eq!(view.status, status);
        assert_eq!(view.paid_amount, Amount::zero());
    }
    request.status = PurchasePaymentRequestStatus::Closed;
    request.lines[0].paid_amount = "20".parse().unwrap();
    request.lines[0].closed_amount = "40".parse().unwrap();
    let view = FulfillmentRequestView::for_purchase(&request, &purchase, "account").unwrap();
    assert_eq!(view.paid_amount, "20".parse().unwrap());
    assert_eq!(view.closed_amount, "40".parse().unwrap());
    assert_eq!(view.remaining_amount, Amount::zero());
}
