//! 申请额度与审批状态边界，不连接外部数据库。
use super::*;
use crate::entity::receivable::{AccountReviewStatus, ReceivableAccount, ReceivableAccountData};
use erp_core::ids::SalesOrderRevisionId;
use std::str::FromStr;

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}
fn request(value: &str) -> SalesInvoiceRequest {
    let account = ReceivableAccount::new(
        ReceivableAccountId::new("account-1"),
        ReceivableAccountData {
            sales_order_id: SalesOrderId::new("sale-1"),
            account_seq: 1,
            customer_id: CustomerAccountId::new("customer-1"),
            counterparty_party_id: PartyId::new("party-1"),
            source_sales_order_revision_id: SalesOrderRevisionId::new("revision-1"),
            review_status: AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: amount("1000"),
            settled_total: Amount::zero(),
            invoiceable_total: amount("1000"),
            invoiced_total: Amount::zero(),
        },
        "sales",
    )
    .unwrap();
    SalesInvoiceRequest::new(
        SalesInvoiceRequestId::new("request-1"),
        &account,
        InvoiceRequestData {
            amount: amount(value),
            invoice_title: "  测试客户  ".into(),
            tax_number: "  TAX-1  ".into(),
            invoice_content: "服务费".into(),
            reason: "按约定开票".into(),
        },
        "sales",
    )
    .unwrap()
}

#[test]
fn submit_and_approve_reserve_without_recording_invoice() {
    let mut req = request("600");
    assert_eq!(req.reserved(), Amount::zero());
    req.submit(amount("1000")).unwrap();
    assert_eq!(req.reserved(), amount("600"));
    assert_eq!(req.approval_subject_version, 1);
    req.approve().unwrap();
    assert_eq!(req.status, InvoiceRequestStatus::Approved);
    assert_eq!(req.invoiced_amount, Amount::zero());
    assert_eq!(req.reserved(), amount("600"));
}
#[test]
fn multiple_requests_cannot_reserve_the_same_capacity() {
    let mut first = request("600");
    first.submit(amount("1000")).unwrap();
    let mut second = request("500");
    let before = second.clone();
    assert!(second
        .submit(amount("1000").checked_sub(first.reserved()))
        .is_err());
    assert_eq!(second, before);
    second.data.amount = amount("400");
    second
        .submit(amount("1000").checked_sub(first.reserved()))
        .unwrap();
    assert_eq!(first.reserved().checked_add(second.reserved()), amount("1000"));
}
#[test]
fn withdrawal_releases_capacity_and_resubmission_advances_version() {
    let mut req = request("600");
    req.submit(amount("1000")).unwrap();
    req.cancel_approval().unwrap();
    assert_eq!(req.reserved(), Amount::zero());
    req.data.amount = amount("300");
    req.submit(amount("400")).unwrap();
    assert_eq!(req.approval_subject_version, 2);
    assert_eq!(req.reserved(), amount("300"));
}
#[test]
fn invoice_requires_approval_and_cannot_exceed_remaining_authorization() {
    let mut req = request("600");
    assert!(req.record_invoice(amount("1")).is_err());
    req.submit(amount("1000")).unwrap();
    assert!(req.record_invoice(amount("1")).is_err());
    req.approve().unwrap();
    req.record_invoice(amount("200")).unwrap();
    let before = req.clone();
    for invalid in ["0", "-1", "400.01"] {
        assert!(req.record_invoice(amount(invalid)).is_err());
        assert_eq!(req, before);
    }
    req.record_invoice(amount("400")).unwrap();
    assert_eq!(req.status, InvoiceRequestStatus::Completed);
    assert_eq!(req.invoiced_amount, amount("600"));
    assert_eq!(req.reserved(), Amount::zero());
    assert!(req.record_invoice(amount("1")).is_err());
}
#[test]
fn duplicate_transitions_and_approved_withdrawal_are_rejected_without_mutation() {
    let mut req = request("600");
    req.submit(amount("1000")).unwrap();
    let pending = req.clone();
    assert!(req.submit(amount("1000")).is_err());
    assert_eq!(req, pending);
    req.approve().unwrap();
    let approved = req.clone();
    assert!(req.approve().is_err());
    assert!(req.cancel_approval().is_err());
    assert_eq!(req, approved);
}
#[test]
fn restored_account_balance_requires_a_new_request_after_red_invoice() {
    let mut old = request("600");
    old.submit(amount("1000")).unwrap();
    old.approve().unwrap();
    old.record_invoice(amount("600")).unwrap();
    // 红冲只恢复账户的未开票余额，不修改旧申请的已执行金额。
    let restored_open = amount("1000");
    assert!(old.record_invoice(amount("600")).is_err());
    let mut new = request("600");
    new.submit(restored_open.checked_sub(old.reserved())).unwrap();
    assert_eq!(new.status, InvoiceRequestStatus::InApproval);
    assert!(new.record_invoice(amount("600")).is_err());
}
#[test]
fn invalid_application_data_is_rejected_and_text_is_normalized() {
    assert!(Amount::from_str("1.001").is_err());
    let valid = request("1.01").data;
    assert_eq!(valid.invoice_title, "测试客户");
    assert_eq!(valid.tax_number, "TAX-1");
    for value in ["0", "-1"] {
        let mut data = valid.clone();
        data.amount = amount(value);
        assert!(data.normalize().is_err());
    }
    let mut blank = valid.clone();
    blank.tax_number = "  ".into();
    assert!(blank.normalize().is_err());
    let mut long = valid;
    long.invoice_title = "字".repeat(257);
    assert!(long.normalize().is_err());
}
#[test]
fn exhausted_approval_version_preserves_draft() {
    let mut req = request("1");
    req.approval_subject_version = u32::MAX;
    let before = req.clone();
    assert!(req.submit(amount("1000")).is_err());
    assert_eq!(req, before);
}
