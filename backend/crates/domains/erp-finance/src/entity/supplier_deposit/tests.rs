use std::str::FromStr;

use entity_core::BaseModel;
use erp_core::common::time::Instant;

use super::*;

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}
fn account() -> SupplierDepositAccount {
    SupplierDepositAccount::new(
        "account".into(),
        DepositAccountIdentity {
            payer_party_id: "payer".into(),
            supplier_id: "supplier".into(),
            external_account_ref: "jd:account".into(),
            currency: DepositCurrency::CNY,
        },
    )
    .unwrap()
}
fn batch() -> SupplierDepositBatch {
    SupplierDepositBatch::new(
        "batch".into(),
        "account".into(),
        amount("10000"),
        DepositRechargeEvidence {
            approved_request_id: "recharge".into(),
            approval_reference: "approval".into(),
            supplier_payment_id: "payment".into(),
            bank_reference: "bank".into(),
            bank_receipt_asset_id: "receipt".into(),
            external_transaction_id: "credit".into(),
            external_evidence_reference: "evidence".into(),
            received_at: Instant::from_unix_secs(1),
        },
    )
    .unwrap()
}
pub(super) fn reservation() -> SupplierDepositReservation {
    SupplierDepositReservation {
        base: BaseModel::new("reserve".into()),
        account_id: "account".into(),
        operation_key: "operation".into(),
        payload_fingerprint: "fingerprint".into(),
        basis: DepositReservationBasis {
            payment_request_id: "request".into(),
            payment_request_version: 1,
            payable_account_id: "payable".into(),
            purchase_order_id: "purchase".into(),
            purchase_revision_id: "revision".into(),
            execution_group_id: "execution".into(),
            approved_goods_amount: amount("1000"),
            approved_freight: amount("20"),
            authorization_id: "grant".into(),
            authorization_version: 1,
        },
        amount: amount("1020"),
        consumed_amount: Amount::zero(),
        released_amount: Amount::zero(),
        batches: vec![DepositBatchAllocation { batch_id: "batch".into(), amount: amount("1020") }],
        exposure: DepositExposure::NotSent,
        clearance_evidence_reference: None,
        actor_id: "system".into(),
        release_receipts: Default::default(),
    }
}

#[test]
fn verified_tail_release_requires_settlement_and_full_refund_for_cancelled_orders() {
    let mut reservation = reservation();
    reservation.consumed_amount = amount("1000");
    let mut usage = SupplierDepositUsage {
        base: BaseModel::new("usage".into()),
        account_id: "account".into(),
        reservation_id: "reserve".into(),
        ledger_entry_id: "ledger".into(),
        amount: amount("1000"),
        settled_amount: amount("1000"),
        returned_amount: Amount::zero(),
        reversed_settlement_amount: Amount::zero(),
        batches: vec![DepositBatchAllocation { batch_id: "batch".into(), amount: amount("1000") }],
        payment_allocation_ids: vec!["allocation".into()],
    };
    assert_eq!(
        reservation.verified_release_amount(&[usage.clone()], DepositReleaseReason::ChargesSettled).unwrap(),
        amount("20")
    );
    assert!(
        reservation
            .verified_release_amount(&[usage.clone()], DepositReleaseReason::CancelledAndReturned)
            .is_err()
    );
    usage.settled_amount = amount("900");
    assert!(
        reservation.verified_release_amount(&[usage.clone()], DepositReleaseReason::ChargesSettled).is_err()
    );
    usage.settled_amount = amount("1000");
    usage.returned_amount = amount("1000");
    usage.reversed_settlement_amount = amount("1000");
    assert_eq!(
        reservation
            .verified_release_amount(&[usage.clone()], DepositReleaseReason::CancelledAndReturned)
            .unwrap(),
        amount("20")
    );
    usage.account_id = "other".into();
    assert!(
        reservation.verified_release_amount(&[usage], DepositReleaseReason::CancelledAndReturned).is_err()
    );
    assert!(reservation.verified_release_amount(&[], DepositReleaseReason::FailedWithoutCharge).is_err());
}

#[test]
fn reservation_consumption_does_not_count_occupancy_twice() {
    let mut account = account();
    let mut batch = batch();
    account.credit(amount("10000")).unwrap();
    account.reserve(amount("1020")).unwrap();
    batch.reserve(amount("1020")).unwrap();
    assert_eq!(account.book_balance, amount("10000"));
    assert_eq!(account.available().unwrap(), amount("8980"));
    account.debit(amount("1020"), amount("1020")).unwrap();
    batch.consume(amount("1020")).unwrap();
    assert_eq!(account.book_balance, amount("8980"));
    assert_eq!(account.reserved_amount, Amount::zero());
    assert_eq!(account.available().unwrap(), amount("8980"));
    assert_eq!(batch.available().unwrap(), amount("8980"));
    account.credit(amount("500")).unwrap();
    batch.record_return(amount("500")).unwrap();
    assert_eq!(account.available().unwrap(), amount("9480"));
    assert_eq!(batch.available().unwrap(), amount("9480"));
    assert!(batch.record_return(amount("521")).is_err());
}

#[test]
fn batch_reservation_preserves_source_order_and_skips_exhausted_batches() {
    let mut batches = vec![batch(), batch(), batch()];
    for (index, batch) in batches.iter_mut().enumerate() {
        batch.base.id = format!("batch-{index}");
    }
    batches[0].reserve(amount("10000")).unwrap();
    batches[1].reserve(amount("9999.99")).unwrap();
    let allocations = DepositBatchAllocation::reserve_from(&mut batches, amount("0.03")).unwrap();
    assert_eq!(
        allocations,
        vec![
            DepositBatchAllocation { batch_id: "batch-1".into(), amount: amount("0.01") },
            DepositBatchAllocation { batch_id: "batch-2".into(), amount: amount("0.02") },
        ]
    );
    assert_eq!(batches[0].reserved_amount, amount("10000"));
    assert_eq!(batches[1].available().unwrap(), Amount::zero());
    assert_eq!(batches[2].reserved_amount.to_string(), "0.02");
}

#[test]
fn batch_reservation_preserves_insufficient_and_corrupt_history_errors() {
    let error = DepositBatchAllocation::reserve_from(&mut [batch()], amount("10000.01")).unwrap_err();
    assert!(matches!(error, crate::Error::BusinessLogicError(ref text) if text == "到账批次可分配额度不足"));
    let mut damaged = batch();
    damaged.reserved_amount = amount("10001");
    let error = DepositBatchAllocation::reserve_from(&mut [damaged.clone()], amount("0.01")).unwrap_err();
    assert!(matches!(error, crate::Error::Logic(ref error) if error.to_string() == "充值批次余额异常"));
    // 已全额分配时不读取后续批次；历史损坏不扩大到原先未访问的来源。
    let allocations = DepositBatchAllocation::reserve_from(&mut [batch(), damaged], amount("10000")).unwrap();
    assert_eq!(allocations.len(), 1);
    assert_eq!(allocations[0].amount, amount("10000"));
    assert!(DepositBatchAllocation::reserve_from(&mut [], Amount::zero()).unwrap().is_empty());
}

#[test]
fn unknown_write_never_releases_on_timeout_or_authorization_revocation() {
    let mut value = reservation();
    value.mark_sent().unwrap();
    let before = value.clone();
    assert!(value.release(amount("1020")).is_err());
    assert_eq!(value, before);
    assert!(value.mark_sent().is_err());
    assert!(value.clear_exposure(" ".into()).is_err());
    value.consume(amount("1000")).unwrap();
    value.clear_exposure("no-pending-debits-or-freezes".into()).unwrap();
    value.release(amount("20")).unwrap();
    assert_eq!(value.remaining().unwrap(), Amount::zero());
}

#[test]
fn real_excess_debit_is_preserved_and_blocks_new_reservations() {
    let mut value = account();
    value.credit(amount("1000")).unwrap();
    value.reserve(amount("100")).unwrap();
    value.debit(amount("120"), amount("100")).unwrap();
    assert_eq!(value.book_balance, amount("880"));
    assert_eq!(value.reserved_amount, Amount::zero());
    assert!(value.reconciliation_required);
    assert!(value.reserve(amount("1")).is_err());
    value.credit(amount("20")).unwrap();
    assert_eq!(value.book_balance, amount("900"));
}

#[test]
fn authorization_has_real_limits_scope_and_independent_terms_version() {
    let mut authorization = SupplierDepositAuthorization {
        mode: Default::default(),
        base: BaseModel::new("grant".into()),
        account_id: "account".into(),
        authorized_by: "finance".into(),
        approval_reference: "approval".into(),
        grant_version: 7,
        valid_from: Instant::from_unix_secs(1),
        valid_until: Instant::from_unix_secs(100),
        per_execution_limit: amount("100"),
        cumulative_limit: amount("150"),
        committed_amount: Amount::zero(),
        purchase_order_ids: vec!["purchase".into()],
        revoked: false,
    };
    authorization.commit("account", "purchase", amount("100"), 7, Instant::from_unix_secs(2)).unwrap();
    authorization.base.version += 1;
    authorization.ensure_active("account", "purchase", 7, Instant::from_unix_secs(3)).unwrap();
    assert!(
        authorization.commit("account", "purchase", amount("51"), 7, Instant::from_unix_secs(3)).is_err()
    );
    assert!(authorization.commit("other", "purchase", amount("1"), 7, Instant::from_unix_secs(3)).is_err());
    assert!(authorization.ensure_active("account", "purchase", 6, Instant::from_unix_secs(3)).is_err());
    assert!(authorization.ensure_active("account", "purchase", 7, Instant::from_unix_secs(100)).is_err());
    authorization.revoked = true;
    assert!(authorization.ensure_active("account", "purchase", 7, Instant::from_unix_secs(3)).is_err());
    assert_eq!(authorization.committed_amount, amount("100"));
}

#[test]
fn incomplete_recharge_and_negative_budget_are_rejected() {
    let mut evidence = batch().evidence;
    evidence.bank_reference.clear();
    assert!(SupplierDepositBatch::new("other".into(), "account".into(), amount("1"), evidence).is_err());
    let mut basis = reservation().basis;
    assert_eq!(basis.total().unwrap(), amount("1020"));
    basis.approved_freight = amount("-1");
    assert!(basis.total().is_err());
}

#[test]
fn persisted_account_and_reservation_roundtrip_keeps_honest_amounts() {
    let mut account = account();
    account.credit(amount("0.01")).unwrap();
    account.reserve(amount("0.01")).unwrap();
    let json = serde_json::to_value(&account).unwrap();
    assert_eq!(json["book_balance"], "0.01");
    assert_eq!(serde_json::from_value::<SupplierDepositAccount>(json).unwrap(), account);
    let original = reservation();
    assert_eq!(
        serde_json::from_str::<SupplierDepositReservation>(&serde_json::to_string(&original).unwrap())
            .unwrap(),
        original
    );
}

#[test]
fn recharge_approval_is_independent_and_does_not_create_available_funds() {
    let mut request = SupplierDepositRechargeRequest::new(
        "request".into(),
        "account".into(),
        "operation".into(),
        "payload".into(),
        amount("100"),
        "合同预存款充值".into(),
        "applicant".into(),
    )
    .unwrap();
    let before = request.clone();
    assert!(request.approve("applicant", "approval".into(), Instant::from_unix_secs(2)).is_err());
    assert_eq!(request, before);
    assert!(request.approve("finance", " ".into(), Instant::from_unix_secs(2)).is_err());
    request.approve("finance", "approval".into(), Instant::from_unix_secs(2)).unwrap();
    assert_eq!(request.status, DepositRechargeStatus::Approved);
    assert_eq!(request.approved_by.as_deref(), Some("finance"));
    assert!(request.supplier_payment_id.is_none());
    assert!(request.received_batch_id.is_none());
}

#[test]
fn funds_summary_keeps_occupancy_and_verified_settlement_separate() {
    let mut account = account();
    account.credit(amount("10000")).unwrap();
    account.reserve(amount("1020")).unwrap();
    let mut reservation = reservation();
    let summary =
        DepositExecutionFundsSummary::from_records("execution", Some(&reservation), &[], Some(&account))
            .unwrap();
    assert_eq!(summary.status, DepositExecutionFundsStatus::Reserved);
    assert_eq!(summary.reserved_amount, amount("1020"));
    assert_eq!(summary.settled_amount, Amount::zero());
    reservation.mark_sent().unwrap();
    let summary =
        DepositExecutionFundsSummary::from_records("execution", Some(&reservation), &[], Some(&account))
            .unwrap();
    assert_eq!(summary.status, DepositExecutionFundsStatus::AwaitingDebitVerification);
    assert_eq!(summary.verified_debit_amount, Amount::zero());
    assert!(summary.pending_return_amount.is_none());
    assert!(
        DepositExecutionFundsSummary::from_records("other", Some(&reservation), &[], Some(&account)).is_err()
    );
}

#[test]
fn purchase_replacement_releases_only_never_sent_or_independently_cleared_funds() {
    let mut account = account();
    account.credit(amount("10000")).unwrap();
    account.reserve(amount("1020")).unwrap();
    let mut original = reservation();
    assert_eq!(original.replacement_release_amount(false, &account, &[]).unwrap(), amount("1020"));
    assert!(original.replacement_release_amount(true, &account, &[]).is_err());
    original.mark_sent().unwrap();
    assert!(original.replacement_release_amount(false, &account, &[]).is_err());
    original.clear_exposure("verified-no-pending-debit-or-freeze".into()).unwrap();
    assert_eq!(original.replacement_release_amount(true, &account, &[]).unwrap(), amount("1020"));
    account.reconciliation_required = true;
    assert!(original.replacement_release_amount(true, &account, &[]).is_err());
}

#[test]
fn purchase_replacement_requires_cash_return_and_settlement_reversal_separately() {
    let mut account = account();
    account.credit(amount("10000")).unwrap();
    account.reserve(amount("1020")).unwrap();
    let mut original = reservation();
    original.mark_sent().unwrap();
    original.consume(amount("1000")).unwrap();
    let mut usage = SupplierDepositUsage {
        base: BaseModel::new("usage".into()),
        account_id: "account".into(),
        reservation_id: "reserve".into(),
        ledger_entry_id: "debit".into(),
        amount: amount("1000"),
        settled_amount: amount("1000"),
        returned_amount: Amount::zero(),
        reversed_settlement_amount: Amount::zero(),
        batches: vec![DepositBatchAllocation { batch_id: "batch".into(), amount: amount("1000") }],
        payment_allocation_ids: vec!["allocation".into()],
    };
    assert!(original.replacement_release_amount(true, &account, std::slice::from_ref(&usage)).is_err());
    usage.returned_amount = amount("1000");
    assert!(original.replacement_release_amount(true, &account, std::slice::from_ref(&usage)).is_err());
    usage.reversed_settlement_amount = amount("1000");
    assert!(original.replacement_release_amount(true, &account, std::slice::from_ref(&usage)).is_err());
    original.clear_exposure("financial-finality-proof".into()).unwrap();
    assert_eq!(original.replacement_release_amount(true, &account, &[usage]).unwrap(), amount("20"));
}
