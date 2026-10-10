use std::str::FromStr;

use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use serde_json::json;

use super::*;

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

fn data(values: &[(&str, &str)]) -> PurchasePaymentRequestData {
    PurchasePaymentRequestData {
        supplier_id: "supplier-1".into(),
        payee_bank_account_id: "bank-1".into(),
        lines: values
            .iter()
            .map(|(id, value)| PaymentRequestLineData {
                payable_account_id: (*id).into(),
                amount: amount(value),
            })
            .collect(),
        desired_payment_date: BusinessDate::from_ymd(2026, 10, 10).unwrap(),
        reason: " 已完成采购对账 ".into(),
        attachment_ids: vec!["attachment-1".into()],
    }
}

fn draft(values: &[(&str, &str)]) -> PurchasePaymentRequest {
    PurchasePaymentRequest::new("request-1".into(), data(values), "buyer-1").unwrap()
}

fn submission(request: &PurchasePaymentRequest) -> PaymentRequestSubmission {
    PaymentRequestSubmission {
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
        lines: request
            .data
            .lines
            .iter()
            .enumerate()
            .map(|(index, data)| PurchasePaymentRequestLine {
                payable_account_id: data.payable_account_id.clone(),
                purchase_order_id: format!("purchase-{index}"),
                purchase_revision_id: format!("revision-{index}"),
                requested_amount: data.amount,
                paid_amount: Amount::zero(),
                closed_amount: Amount::zero(),
                gross_total: amount("100000"),
                settled_total: amount("30000"),
                open_total: amount("70000"),
                other_reserved_total: Amount::zero(),
                payment_terms: json!({"type":"monthly"}),
                source_snapshot: PaymentRequestSourceSnapshot {
                    purchase_preview: json!({"revision_id":format!("revision-{index}")}),
                    sales_sources: vec![PaymentRequestSalesSource {
                        sales_order_id: "sales-1".into(),
                        sales_revision_id: "sales-revision-1".into(),
                        customer_id: "customer-1".into(),
                        payer_party_id: "company-1".into(),
                        payer_party_name: "我方公司".into(),
                        preview: json!({"order_no":"XS-1"}),
                    }],
                    allowed_attachment_ids: vec!["attachment-1".into()],
                },
            })
            .collect(),
    }
}

fn approved(values: &[(&str, &str)]) -> PurchasePaymentRequest {
    let mut request = draft(values);
    request.submit(submission(&request)).unwrap();
    request.approve().unwrap();
    request
}

fn allocations(values: &[(&str, &str)]) -> Vec<PaymentRequestAllocation> {
    values
        .iter()
        .map(|(id, value)| PaymentRequestAllocation {
            payable_account_id: (*id).into(),
            amount: amount(value),
        })
        .collect()
}

#[test]
fn draft_is_normalized_unreserved_and_server_sums_each_purchase() {
    let request = draft(&[("account-1", "12.34"), ("account-2", "0.01")]);
    assert_eq!(request.data.reason, "已完成采购对账");
    assert_eq!(request.total_amount, amount("12.35"));
    assert_eq!(request.reserved_for("account-1"), Amount::zero());
    assert_eq!(request.submission_version, 0);
    assert!(request.payee_account.is_none());
}

#[test]
fn showing_validated_draft_basis_neither_reserves_nor_changes_approval_identity() {
    let mut request = draft(&[("account-1", "70")]);
    request.set_draft_basis(submission(&request)).unwrap();
    assert_eq!(request.payer_party_id, "company-1");
    assert_eq!(request.total_amount, amount("70"));
    assert_eq!(request.status, PurchasePaymentRequestStatus::Draft);
    assert_eq!(request.reserved_for("account-1"), Amount::zero());
    assert_eq!(request.submission_version, 0);
    assert!(request.approval_instance_id.is_none());
    request.submit(submission(&request)).unwrap();
    request.approval_instance_id = Some("old-instance".into());
    request.reject("核对金额").unwrap();
    request.set_draft_basis(submission(&request)).unwrap();
    assert_eq!(request.status, PurchasePaymentRequestStatus::Rejected);
    assert_eq!(request.submission_version, 1);
    assert_eq!(request.approval_instance_id.as_deref(), Some("old-instance"));
    assert_eq!(request.reserved_for("account-1"), Amount::zero());
}

#[test]
fn form_rejects_zero_negative_duplicates_empty_reason_and_overflow() {
    for values in
        [&[("account-1", "0")][..], &[("account-1", "-0.01")], &[("account-1", "1"), ("account-1", "2")], &[]]
    {
        assert!(PurchasePaymentRequest::new("id".into(), data(values), "buyer").is_err());
    }
    let mut blank = data(&[("account-1", "1")]);
    blank.reason = " ".into();
    assert!(PurchasePaymentRequest::new("id".into(), blank, "buyer").is_err());
    let max = "79228162514264337593543950335";
    assert!(PurchasePaymentRequest::new("id".into(), data(&[("a", max), ("b", max)]), "buyer").is_err());
}

#[test]
fn submission_checks_source_subject_supplier_account_and_available_before_mutation() {
    let request = draft(&[("account-1", "70000")]);
    for mutation in 0..11 {
        let mut candidate = request.clone();
        let mut frozen = submission(&candidate);
        match mutation {
            0 => frozen.supplier_id = "other-supplier".into(),
            1 => frozen.payee_account.account_id = "other-bank".into(),
            2 => frozen.payer_party_id.clear(),
            3 => frozen.lines[0].source_snapshot.sales_sources[0].payer_party_id = "other-company".into(),
            4 => frozen.lines[0].other_reserved_total = amount("0.01"),
            5 => frozen.lines[0].purchase_revision_id.clear(),
            6 => frozen.lines[0].source_snapshot.sales_sources.clear(),
            7 => frozen.lines[0].requested_amount = amount("69999"),
            8 => frozen.payee_account.version = 0,
            9 => frozen.payee_account.account_name.clear(),
            _ => frozen.lines[0].settled_total = amount("29999"),
        }
        assert!(candidate.submit(frozen).is_err(), "mutation {mutation}");
        assert_eq!(candidate, request);
    }
}

#[test]
fn version_overflow_and_reverse_overpaid_are_atomic() {
    let mut draft = draft(&[("account-1", "10")]);
    draft.submission_version = u32::MAX;
    let original = draft.clone();
    assert!(draft.submit(submission(&draft)).is_err());
    assert_eq!(draft, original);
    let mut paid = approved(&[("account-1", "10")]);
    paid.record_payment(&allocations(&[("account-1", "3")])).unwrap();
    let original = paid.clone();
    assert!(paid.reverse_payment(&allocations(&[("account-1", "3.01")])).is_err());
    assert_eq!(paid, original);
}

#[test]
fn reject_withdraw_and_resubmit_release_old_authority_and_freeze_new_version() {
    for reject in [true, false] {
        let mut request = draft(&[("account-1", "70000")]);
        request.submit(submission(&request)).unwrap();
        assert_eq!(request.reserved_for("account-1"), amount("70000"));
        request.approval_instance_id = Some("old-approval".into());
        request.work_item_id = Some("old-task".into());
        if reject {
            request.reject("依据不足").unwrap();
        } else {
            request.withdraw("调整申请").unwrap();
        }
        assert_eq!(request.reserved_for("account-1"), Amount::zero());
        assert!(request.record_payment(&allocations(&[("account-1", "1")])).is_err());
        request.submit(submission(&request)).unwrap();
        assert_eq!(request.submission_version, 2);
        assert_eq!(request.approval_subject_version, 2);
        assert!(request.approval_instance_id.is_none());
        assert!(request.work_item_id.is_none());
        assert_eq!(request.status, PurchasePaymentRequestStatus::InApproval);
    }
}

#[test]
fn contract_100k_30k_paid_70k_request_then_40k_payment_keeps_30k_reserved() {
    let mut request = approved(&[("account-1", "70000")]);
    assert_eq!(request.paid_amount(), Amount::zero());
    request.record_payment(&allocations(&[("account-1", "40000")])).unwrap();
    assert_eq!(request.paid_amount(), amount("40000"));
    assert_eq!(request.remaining(), amount("30000"));
    assert_eq!(request.reserved_for("account-1"), amount("30000"));
    assert_eq!(request.status, PurchasePaymentRequestStatus::PartiallyPaid);
    assert_eq!(request.close("账户发生变更").unwrap(), allocations(&[("account-1", "30000")]));
    assert_eq!(request.paid_amount(), amount("40000"));
    assert_eq!(request.closed_amount(), amount("30000"));
    assert_eq!(request.reserved_for("account-1"), Amount::zero());
    assert_eq!(request.status, PurchasePaymentRequestStatus::Closed);
}

#[test]
fn partial_payments_cannot_move_between_lines_or_mutate_on_failure() {
    let request = approved(&[("account-1", "10"), ("account-2", "20")]);
    for values in [
        allocations(&[("account-1", "10.01")]),
        allocations(&[("account-1", "1"), ("account-1", "1")]),
        allocations(&[("unknown", "1")]),
        allocations(&[("account-2", "20"), ("account-1", "11")]),
    ] {
        let mut candidate = request.clone();
        assert!(candidate.record_payment(&values).is_err());
        assert_eq!(candidate, request);
    }
    let mut paid = request;
    paid.record_payment(&allocations(&[("account-1", "10"), ("account-2", "20")])).unwrap();
    assert_eq!(paid.status, PurchasePaymentRequestStatus::Paid);
    assert_eq!(paid.remaining(), Amount::zero());
    assert!(paid.update(data(&[("account-1", "20")])).is_err());
}

#[test]
fn closing_fully_paid_authority_keeps_history_and_reversal_never_restores_reservation() {
    let mut request = approved(&[("account-1", "10.01"), ("account-2", "20.02")]);
    request.work_item_id = Some("completed-task".into());
    request.record_payment(&allocations(&[("account-1", "10.01"), ("account-2", "20.02")])).unwrap();
    assert!(request.close(" 收款账户已维护，终止原授权 ").unwrap().is_empty());
    assert_eq!(request.status, PurchasePaymentRequestStatus::Closed);
    assert_eq!(request.paid_amount(), amount("30.03"));
    assert_eq!(request.closed_amount(), Amount::zero());
    assert_eq!(request.remaining(), Amount::zero());
    assert_eq!(request.work_item_id.as_deref(), Some("completed-task"));
    assert_eq!(request.last_reason.as_deref(), Some("收款账户已维护，终止原授权"));
    for (account, value) in [("account-1", "10.01"), ("account-2", "20.02")] {
        request.reverse_payment(&allocations(&[(account, value)])).unwrap();
        assert_eq!(request.status, PurchasePaymentRequestStatus::Closed);
        assert_eq!(request.remaining(), Amount::zero());
        assert_eq!(request.reserved_for(account), Amount::zero());
        assert_eq!(request.paid_amount().checked_add(request.closed_amount()), request.total_amount);
        assert_eq!(request.work_item_id.as_deref(), Some("completed-task"));
    }
    assert_eq!(request.paid_amount(), Amount::zero());
    assert_eq!(request.closed_amount(), amount("30.03"));
    assert!(request.record_payment(&allocations(&[("account-1", "0.01")])).is_err());
}

#[test]
fn close_rejects_nonapproved_states_or_missing_reason_before_mutation() {
    for status in [
        PurchasePaymentRequestStatus::Draft,
        PurchasePaymentRequestStatus::InApproval,
        PurchasePaymentRequestStatus::Rejected,
        PurchasePaymentRequestStatus::Withdrawn,
        PurchasePaymentRequestStatus::Closed,
    ] {
        assert!(!status.can_close());
        let mut request = approved(&[("account-1", "10")]);
        request.status = status;
        let original = request.clone();
        let error = request.close("终止原授权").unwrap_err();
        assert!(error.to_string().contains("只有已批准、部分付款或已付清"));
        assert_eq!(request, original);
    }
    for status in [
        PurchasePaymentRequestStatus::Approved,
        PurchasePaymentRequestStatus::PartiallyPaid,
        PurchasePaymentRequestStatus::Paid,
    ] {
        assert!(status.can_close());
        let mut request = approved(&[("account-1", "10")]);
        request.status = status;
        let original = request.clone();
        let error = request.close(" ").unwrap_err();
        assert!(error.to_string().contains("关闭原因"));
        assert_eq!(request, original);
    }
}

#[test]
fn reversal_restores_open_request_only_and_closed_request_stays_closed() {
    let mut request = approved(&[("account-1", "70")]);
    request.record_payment(&allocations(&[("account-1", "70")])).unwrap();
    request.reverse_payment(&allocations(&[("account-1", "40")])).unwrap();
    assert_eq!(request.reserved_for("account-1"), amount("40"));
    assert_eq!(request.status, PurchasePaymentRequestStatus::PartiallyPaid);
    request.close("终止未付").unwrap();
    request.reverse_payment(&allocations(&[("account-1", "10")])).unwrap();
    assert_eq!(request.status, PurchasePaymentRequestStatus::Closed);
    assert_eq!(request.paid_amount(), amount("20"));
    assert_eq!(request.closed_amount(), amount("50"));
    assert_eq!(request.reserved_for("account-1"), Amount::zero());
    assert!(request.record_payment(&allocations(&[("account-1", "1")])).is_err());
    request.reverse_payment(&allocations(&[("account-1", "20")])).unwrap();
    assert_eq!(request.status, PurchasePaymentRequestStatus::Closed);
    assert_eq!(request.paid_amount(), Amount::zero());
    assert_eq!(request.closed_amount(), amount("70"));
    assert_eq!(request.reserved_for("account-1"), Amount::zero());
    assert!(request.record_payment(&allocations(&[("account-1", "1")])).is_err());
}

#[test]
fn every_nonapproved_state_denies_payment_and_closed_cannot_reopen() {
    for status in [
        PurchasePaymentRequestStatus::Draft,
        PurchasePaymentRequestStatus::InApproval,
        PurchasePaymentRequestStatus::Rejected,
        PurchasePaymentRequestStatus::Withdrawn,
        PurchasePaymentRequestStatus::Paid,
        PurchasePaymentRequestStatus::Closed,
    ] {
        let mut request = approved(&[("account-1", "10")]);
        request.status = status;
        let original = request.clone();
        assert!(request.record_payment(&allocations(&[("account-1", "1")])).is_err());
        assert_eq!(request, original);
    }
}

#[test]
fn submitted_materials_remain_exact_after_withdrawal_and_resubmission() {
    let mut request = draft(&[("account-1", "70")]);
    assert!(PurchasePaymentRequestSubmissionSnapshot::freeze(&request).is_err());
    request.submit(submission(&request)).unwrap();
    assert!(PurchasePaymentRequestSubmissionSnapshot::freeze(&request).is_err());
    request.approval_instance_id = Some("approval-1".into());
    let original = PurchasePaymentRequestSubmissionSnapshot::freeze(&request).unwrap();
    request.withdraw("核对新的对账依据").unwrap();
    request.update(data(&[("account-1", "20")])).unwrap();
    request.submit(submission(&request)).unwrap();
    request.approval_instance_id = Some("approval-2".into());
    let next = PurchasePaymentRequestSubmissionSnapshot::freeze(&request).unwrap();
    request.lines[0].source_snapshot.sales_sources[0].preview = json!({"order_no":"changed-current"});
    assert_eq!(original.submission_version, 1);
    assert_eq!(original.approval_instance_id, "approval-1");
    assert_eq!(original.request.total_amount, amount("70"));
    assert_eq!(
        original.request.lines[0].source_snapshot.sales_sources[0].preview,
        json!({"order_no":"XS-1"})
    );
    assert_eq!(next.submission_version, 2);
    assert_eq!(next.approval_instance_id, "approval-2");
    assert_eq!(next.request.total_amount, amount("20"));
    assert_ne!(original.base.id, next.base.id);
    let decoded: PurchasePaymentRequestSubmissionSnapshot =
        serde_json::from_value(serde_json::to_value(&original).unwrap()).unwrap();
    assert_eq!(decoded, original);
}
