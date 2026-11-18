use std::str::FromStr;

use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use serde_json::json;

use super::*;
use crate::entity::supplier_deposit::{DepositAccountIdentity, DepositCurrency, DepositReservationBasis};

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

fn data(values: &[(&str, &str)]) -> PurchasePaymentRequestData {
    PurchasePaymentRequestData {
        execution_method: Default::default(),
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
        deposit_account: None,
        payee_account: Some(PaymentRequestAccountSnapshot {
            account_id: "bank-1".into(),
            version: 1,
            supplier_party_id: "supplier-party".into(),
            account_name: "供应商".into(),
            bank_name: "开户行".into(),
            masked_account_number: "****1234".into(),
        }),
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
            1 => frozen.payee_account.as_mut().unwrap().account_id = "other-bank".into(),
            2 => frozen.payer_party_id.clear(),
            3 => frozen.lines[0].source_snapshot.sales_sources[0].payer_party_id = "other-company".into(),
            4 => frozen.lines[0].other_reserved_total = amount("0.01"),
            5 => frozen.lines[0].purchase_revision_id.clear(),
            6 => frozen.lines[0].source_snapshot.sales_sources.clear(),
            7 => frozen.lines[0].requested_amount = amount("69999"),
            8 => frozen.payee_account.as_mut().unwrap().version = 0,
            9 => frozen.payee_account.as_mut().unwrap().account_name.clear(),
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

fn deposit_draft() -> PurchasePaymentRequest {
    let mut form = data(&[("account-1", "100")]);
    form.execution_method = PaymentRequestExecutionMethod::SupplierDeposit { account_id: "deposit-1".into() };
    form.payee_bank_account_id.clear();
    PurchasePaymentRequest::new("deposit-request".into(), form, "buyer").unwrap()
}

fn deposit_submission(request: &PurchasePaymentRequest) -> PaymentRequestSubmission {
    let mut frozen = submission(request);
    frozen.payee_account = None;
    frozen.deposit_account = Some(PaymentRequestDepositAccountSnapshot {
        account_id: "deposit-1".into(),
        identity: DepositAccountIdentity {
            payer_party_id: "company-1".into(),
            supplier_id: "supplier-1".into(),
            external_account_ref: "server-account-ref".into(),
            currency: DepositCurrency::CNY,
        },
    });
    frozen
}

#[test]
fn deposit_request_preserves_approval_and_never_executes_as_bank_payment() {
    let mut request = deposit_draft();
    request.submit(deposit_submission(&request)).unwrap();
    request.approval_instance_id = Some("approval".into());
    let material = PurchasePaymentRequestSubmissionSnapshot::freeze(&request).unwrap();
    assert!(material.request.payee_account.is_none());
    assert_eq!(material.request.deposit_account.as_ref().unwrap().account_id, "deposit-1");

    request.approve().unwrap();
    assert!(request.require_deposit_account("deposit-1").is_ok());
    assert!(request.require_deposit_account("other").is_err());
    request.reserve_deposit("reservation", "account-1", 1, amount("100")).unwrap();
    assert_eq!(request.paid_amount(), Amount::zero());
    assert_eq!(request.available_to_execute("account-1").unwrap(), Amount::zero());

    let before = request.clone();
    assert!(request.record_payment(&allocations(&[("account-1", "1")])).is_err());
    assert_eq!(request, before);

    request.settle_deposit("reservation", amount("40")).unwrap();
    assert_eq!(request.paid_amount(), amount("40"));
    assert_eq!(request.deposit_reservations["reservation"].amount, amount("60"));
    assert_eq!(request.available_to_execute("account-1").unwrap(), Amount::zero());

    let before = request.clone();
    assert!(request.settle_deposit("reservation", amount("61")).is_err());
    assert_eq!(request, before);
    assert!(request.close("原扣款待核实").is_err());
}

#[test]
fn deposit_submission_rejects_mixed_bank_and_cross_owner_while_legacy_defaults_to_bank() {
    let mut value = serde_json::to_value(data(&[("account-1", "100")])).unwrap();
    value.as_object_mut().unwrap().remove("execution_method");
    let legacy: PurchasePaymentRequestData = serde_json::from_value(value).unwrap();
    assert_eq!(legacy.execution_method, PaymentRequestExecutionMethod::BankTransfer);
    assert!(serde_json::to_value(&legacy).unwrap().get("execution_method").is_none());

    let mut mixed = deposit_draft().data;
    mixed.payee_bank_account_id = "bank-1".into();
    assert!(mixed.normalize().is_err());

    let mut request = deposit_draft();
    let mut frozen = deposit_submission(&request);
    frozen.deposit_account.as_mut().unwrap().identity.payer_party_id = "other-company".into();
    let original = request.clone();
    assert!(request.submit(frozen).is_err());
    assert_eq!(request, original);

    let mut frozen = deposit_submission(&request);
    frozen.payee_account = submission(&request).payee_account;
    assert!(request.submit(frozen).is_err());
}

#[test]
fn original_deposit_basis_accepts_paid_requests_and_rejects_closed_or_changed_sources() {
    let mut request = deposit_draft();
    request.submit(deposit_submission(&request)).unwrap();
    request.approve().unwrap();
    let identity = request.deposit_account.as_ref().unwrap().identity.clone();
    let mut basis = DepositReservationBasis {
        payment_request_id: request.base.id.clone(),
        payment_request_version: request.submission_version,
        payable_account_id: "account-1".into(),
        purchase_order_id: "purchase-0".into(),
        purchase_revision_id: "revision-0".into(),
        execution_group_id: "execution".into(),
        approved_goods_amount: amount("100"),
        approved_freight: Amount::zero(),
        authorization_id: "grant".into(),
        authorization_version: 1,
    };
    for status in [
        PurchasePaymentRequestStatus::Approved,
        PurchasePaymentRequestStatus::PartiallyPaid,
        PurchasePaymentRequestStatus::Paid,
    ] {
        request.status = status;
        assert!(request.ensure_deposit_reservation("deposit-1", &identity, &basis).is_ok());
    }

    for status in [
        PurchasePaymentRequestStatus::Draft,
        PurchasePaymentRequestStatus::InApproval,
        PurchasePaymentRequestStatus::Rejected,
        PurchasePaymentRequestStatus::Withdrawn,
        PurchasePaymentRequestStatus::Closed,
    ] {
        request.status = status;
        let error = request.ensure_deposit_reservation("deposit-1", &identity, &basis).unwrap_err();
        assert!(
            matches!(error, crate::Error::BusinessLogicError(ref text) if text == "原付款申请未批准或授权已关闭")
        );
    }

    request.status = PurchasePaymentRequestStatus::Approved;
    basis.purchase_revision_id = "changed".into();
    let error = request.ensure_deposit_reservation("deposit-1", &identity, &basis).unwrap_err();
    assert!(
        matches!(error, crate::Error::BusinessLogicError(ref text) if text == "资金执行采购修订与原批准来源不一致")
    );

    basis.payment_request_version += 1;
    let error = request.ensure_deposit_reservation("deposit-1", &identity, &basis).unwrap_err();
    assert!(
        matches!(error, crate::Error::BusinessLogicError(ref text) if text == "预存款与原付款申请主体、供应商或审批版本不一致")
    );

    request.deposit_account = None;
    assert!(matches!(
        request.ensure_deposit_reservation("deposit-1", &identity, &basis),
        Err(crate::Error::Logic(_))
    ));
}

#[test]
fn deposit_release_updates_original_reservation_and_removes_only_after_full_release() {
    let mut request = deposit_draft();
    request.submit(deposit_submission(&request)).unwrap();
    request.approve().unwrap();
    request.reserve_deposit("reservation", "account-1", 1, amount("100")).unwrap();
    let before = request.clone();
    assert!(request.release_deposit("missing", amount("1")).is_err());
    assert!(request.release_deposit("reservation", amount("101")).is_err());
    assert_eq!(request, before);

    request.release_deposit("reservation", amount("40")).unwrap();
    assert_eq!(request.deposit_reservations["reservation"].amount, amount("60"));
    assert_eq!(request.available_to_execute("account-1").unwrap(), amount("40"));

    request.release_deposit("reservation", amount("60")).unwrap();
    assert!(!request.deposit_reservations.contains_key("reservation"));
    assert_eq!(request.available_to_execute("account-1").unwrap(), amount("100"));
}

// 回归检查：驳回后保存修改不再清除驳回状态，也不重新取得付款额度。
#[test]
fn rejected_request_edit_preserves_rejection_and_releases_authority() {
    let mut request = draft(&[("account-1", "100")]);
    request.submit(submission(&request)).unwrap();
    request.reject("补充付款依据").unwrap();
    request.update(data(&[("account-1", "80")])).unwrap();
    assert_eq!(request.status, PurchasePaymentRequestStatus::Rejected);
    assert_eq!(request.last_reason.as_deref(), Some("补充付款依据"));
    assert_eq!(request.reserved_for("account-1"), Amount::zero());
    assert_eq!(request.approval_subject_version, 1);
    request.submit(submission(&request)).unwrap();
    assert_eq!(request.approval_subject_version, 2);
    assert_eq!(request.reserved_for("account-1"), amount("80"));
}
