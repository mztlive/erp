use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_core::money::Amount;

use super::*;

fn amount(value: &str) -> Amount {
    value.parse().unwrap()
}

fn archive(reference: &str) -> FundsEvidenceArchive {
    FundsEvidenceArchive {
        reference: reference.into(),
        body_encrypted: "encrypted-source-response".into(),
        content_fingerprint: "source-content-digest".into(),
    }
}

fn transaction(direction: FundsTransactionDirection) -> FundsEvidenceInput {
    FundsEvidenceInput {
        source_observed_at: Instant::from_unix_secs(20),
        source_revision: Some("source-revision".into()),
        archive: archive("archive-original"),
        result: FundsEvidenceResult::Transaction {
            stable_transaction_id: "source-identity".into(),
            external_order_id: Some("supplier-order".into()),
            direction,
            amount: amount("10"),
            currency: DepositCurrency::CNY,
            occurred_at: Instant::from_unix_secs(10),
            evidence_reference: "archive-original".into(),
            original_transaction_id: None,
            bank_reference: None,
            business_code: Some("ORDER_DEBIT".into()),
        },
    }
}

fn unresolved(value: Option<Amount>) -> FundsEvidenceInput {
    FundsEvidenceInput {
        source_observed_at: Instant::from_unix_secs(20),
        source_revision: None,
        archive: archive("archive-unknown"),
        result: FundsEvidenceResult::Unresolved {
            observation_id: "source-identity".into(),
            evidence_reference: "archive-unknown".into(),
            external_transaction_id: Some("source-identity".into()),
            external_order_id: None,
            amount: value,
            occurred_at: None,
            business_code: None,
            reason_code: "BUSINESS_TYPE_UNVERIFIED".into(),
        },
    }
}

fn inbox(input: FundsEvidenceInput) -> FundsEvidenceInbox {
    input.validate().unwrap();
    FundsEvidenceInbox {
        base: BaseModel::fake(),
        account_id: "account".into(),
        source_key: input.key().unwrap(),
        semantic_fingerprint: "semantic-digest".into(),
        state: input.initial_state(),
        input,
        first_observed_at: Instant::from_unix_secs(20),
        last_observed_at: Instant::from_unix_secs(20),
        applied_resource: None,
        review_reason_code: None,
        review_resolution: None,
    }
}

#[test]
fn old_balanced_record_cannot_clear_later_risk_in_reviewed_or_applied_state() {
    let mut reservation = super::tests::reservation();
    reservation.base.created_at = 5;
    let mut record = inbox(transaction(FundsTransactionDirection::Debit));
    record.base.created_at = 30;
    record.base.updated_at = 36;
    record.last_observed_at = Instant::from_unix_secs(35);
    record.input.source_observed_at = Instant::from_unix_secs(35);
    if let FundsEvidenceResult::Transaction { occurred_at, .. } = &mut record.input.result {
        *occurred_at = Instant::from_unix_secs(30);
    }
    record.needs_review("FUNDS_SETTLEMENT_NEEDS_REVIEW").unwrap();

    let mut ledger_base = BaseModel::new("ledger".into());
    ledger_base.created_at = 31;
    ledger_base.updated_at = 32;
    let ledger = SupplierDepositLedgerEntry {
        base: ledger_base,
        account_id: "account".into(),
        operation_key: "debit".into(),
        payload_fingerprint: "payload".into(),
        kind: DepositLedgerKind::Debit,
        amount: amount("10"),
        reservation_id: Some(reservation.base.id.clone()),
        original_usage_id: None,
        batch_allocations: vec![],
        actor_id: "system".into(),
        evidence: DepositTransactionEvidence {
            external_transaction_id: "source-identity".into(),
            external_order_id: "supplier-order".into(),
            evidence_reference: "archive-original".into(),
            occurred_at: Instant::from_unix_secs(30),
        },
    };

    let mut basis = SupplierDepositReconciliation {
        base: BaseModel::new("balanced".into()),
        account_id: "account".into(),
        operation_key: "balanced".into(),
        covered_from: Instant::from_unix_secs(1),
        covered_until: Instant::from_unix_secs(20),
        observed_at: Instant::from_unix_secs(20),
        remote_balance: amount("90"),
        local_balance_at_cutoff: amount("90"),
        unresolved_transaction_ids: vec![],
        evidence_reference: "balance-source".into(),
        resolved_by: None,
        resolution_reference: None,
        resolution: None,
    };
    basis.ensure_resolution_basis(&basis, Instant::from_unix_secs(40)).unwrap();
    assert!(record.ensure_review_coverage(&basis, &ledger, Some(&reservation)).is_err());
    assert_eq!(record.state, FundsEvidenceInboxState::NeedsReview);
    assert!(record.applied_resource.is_none());

    basis.covered_until = Instant::from_unix_secs(40);
    basis.observed_at = Instant::from_unix_secs(40);
    record.ensure_review_coverage(&basis, &ledger, Some(&reservation)).unwrap();
    record
        .resolve_reviewed(
            FundsAppliedResource::DebitUsage { usage_id: "usage".into() },
            FundsEvidenceReviewResolution {
                operation_key: "resolve".into(),
                payload_fingerprint: "proof".into(),
                evidence_asset_id: "asset".into(),
                actor_id: "reviewer".into(),
                reviewed_at: Instant::from_unix_secs(41),
                original_reason_code: None,
            },
        )
        .unwrap();

    let fresh = basis.clone();
    basis.observed_at = Instant::from_unix_secs(34);
    assert!(record.ensure_review_coverage(&basis, &ledger, Some(&reservation)).is_err());
    basis = fresh.clone();
    basis.covered_until = Instant::from_unix_secs(30);
    assert!(record.ensure_review_coverage(&basis, &ledger, Some(&reservation)).is_err());
    basis = fresh;
    basis.covered_from = Instant::from_unix_secs(6);
    assert!(record.ensure_review_coverage(&basis, &ledger, Some(&reservation)).is_err());
}

#[test]
fn reviewed_real_resource_keeps_original_reason_and_can_conclude_message() {
    let account = SupplierDepositAccount::new(
        "account".into(),
        DepositAccountIdentity {
            payer_party_id: "payer".into(),
            supplier_id: "supplier".into(),
            external_account_ref: "verified".into(),
            currency: DepositCurrency::CNY,
        },
    )
    .unwrap();
    let mut record = inbox(transaction(FundsTransactionDirection::Debit));
    record.needs_review("FUNDS_SETTLEMENT_NEEDS_REVIEW").unwrap();
    let proof = FundsEvidenceReviewResolution {
        operation_key: "close".into(),
        payload_fingerprint: "payload".into(),
        evidence_asset_id: "attachment".into(),
        actor_id: "reviewer".into(),
        reviewed_at: Instant::from_unix_secs(30),
        original_reason_code: None,
    };
    record
        .resolve_reviewed(
            FundsAppliedResource::DebitUsage { usage_id: "original-usage".into() },
            proof.clone(),
        )
        .unwrap();
    assert_eq!(
        record.review_resolution.as_ref().unwrap().original_reason_code.as_deref(),
        Some("FUNDS_SETTLEMENT_NEEDS_REVIEW")
    );
    assert_eq!(record.state, FundsEvidenceInboxState::Applied);
    record
        .ensure_message_conclusion(
            &account,
            "source-identity",
            "supplier-order",
            FundsTransactionDirection::Debit,
        )
        .unwrap();

    let mut conflict = inbox(transaction(FundsTransactionDirection::Debit));
    conflict.needs_review("SOURCE_FACT_CONFLICT").unwrap();
    assert!(
        conflict
            .resolve_reviewed(
                FundsAppliedResource::DebitUsage { usage_id: "original-usage".into() },
                proof.clone()
            )
            .is_err()
    );

    let mut unknown = inbox(unresolved(Some(amount("10"))));
    assert!(
        unknown
            .resolve_reviewed(FundsAppliedResource::DebitUsage { usage_id: "original-usage".into() }, proof)
            .is_err()
    );
}

#[test]
fn stable_transactions_and_unresolved_observations_keep_distinct_typed_keys() {
    let known = transaction(FundsTransactionDirection::Debit);
    let unknown = unresolved(None);
    known.validate().unwrap();
    unknown.validate().unwrap();
    assert!(known.key().unwrap() == FundsEvidenceKey::Transaction("source-identity".into()));
    assert!(unknown.key().unwrap() == FundsEvidenceKey::Observation("source-identity".into()));
    assert!(known.key().unwrap() != unknown.key().unwrap());
    let encoded_known = serde_json::to_string(&known.key().unwrap()).unwrap();
    let encoded_unknown = serde_json::to_string(&unknown.key().unwrap()).unwrap();
    assert!(encoded_known != encoded_unknown);
}

#[test]
fn message_conclusion_requires_this_exact_transaction_and_never_uses_historical_same_order() {
    let mut account = SupplierDepositAccount::new(
        "account".into(),
        DepositAccountIdentity {
            payer_party_id: "payer".into(),
            supplier_id: "supplier".into(),
            external_account_ref: "verified".into(),
            currency: DepositCurrency::CNY,
        },
    )
    .unwrap();
    let mut record = inbox(transaction(FundsTransactionDirection::Return));
    assert!(
        record
            .ensure_message_conclusion(
                &account,
                "source-identity",
                "supplier-order",
                FundsTransactionDirection::Return
            )
            .is_err()
    );
    record.apply(FundsAppliedResource::ReturnUsage { usage_id: "original-usage".into() }).unwrap();
    account.enabled = false;
    record
        .ensure_message_conclusion(
            &account,
            "source-identity",
            "supplier-order",
            FundsTransactionDirection::Return,
        )
        .unwrap();
    assert!(
        record
            .ensure_message_conclusion(
                &account,
                "another-refund",
                "supplier-order",
                FundsTransactionDirection::Return
            )
            .is_err()
    );
    assert!(
        record
            .ensure_message_conclusion(
                &account,
                "source-identity",
                "another-order",
                FundsTransactionDirection::Return
            )
            .is_err()
    );
    assert!(
        record
            .ensure_message_conclusion(
                &account,
                "source-identity",
                "supplier-order",
                FundsTransactionDirection::Debit
            )
            .is_err()
    );
    account.reconciliation_required = true;
    assert!(
        record
            .ensure_message_conclusion(
                &account,
                "source-identity",
                "supplier-order",
                FundsTransactionDirection::Return
            )
            .is_err()
    );
}

#[test]
fn semantic_result_ignores_archiving_and_sampling_without_ignoring_financial_changes() {
    let original = transaction(FundsTransactionDirection::Debit);
    let expected = original.semantic_result();
    let mut sampled_again = original.clone();
    sampled_again.source_observed_at = Instant::from_unix_secs(200);
    sampled_again.source_revision = Some("later-sample-version".into());
    sampled_again.archive = FundsEvidenceArchive {
        reference: "another-archive".into(),
        body_encrypted: "another-encryption-nonce".into(),
        content_fingerprint: "another-raw-envelope-digest".into(),
    };
    if let FundsEvidenceResult::Transaction { evidence_reference, .. } = &mut sampled_again.result {
        *evidence_reference = "another-archive".into();
    }
    assert!(sampled_again.semantic_result() == expected);

    for mutation in 0..5 {
        let mut changed = original.clone();
        if let FundsEvidenceResult::Transaction {
            amount: value,
            direction,
            external_order_id,
            original_transaction_id,
            occurred_at,
            ..
        } = &mut changed.result
        {
            match mutation {
                0 => *value = amount("10.01"),
                1 => *direction = FundsTransactionDirection::Return,
                2 => *external_order_id = Some("another-order".into()),
                3 => *original_transaction_id = Some("original-debit".into()),
                _ => *occurred_at = Instant::from_unix_secs(11),
            }
        }
        assert!(changed.semantic_result() != expected, "mutation {mutation}");
    }

    let unknown = unresolved(None);
    let mut rearchived = unknown.clone();
    rearchived.source_observed_at = Instant::from_unix_secs(300);
    rearchived.archive = archive("new-unknown-archive");
    if let FundsEvidenceResult::Unresolved { evidence_reference, .. } = &mut rearchived.result {
        *evidence_reference = "new-unknown-archive".into();
    }
    assert!(unknown.semantic_result() == rearchived.semantic_result());
}

#[test]
fn inbox_applies_only_the_matching_direction_and_same_resource_replays_without_change() {
    for (direction, resource) in [
        (
            FundsTransactionDirection::Recharge,
            FundsAppliedResource::RechargeBatch { batch_id: "batch".into() },
        ),
        (FundsTransactionDirection::Debit, FundsAppliedResource::DebitUsage { usage_id: "debit".into() }),
        (FundsTransactionDirection::Return, FundsAppliedResource::ReturnUsage { usage_id: "return".into() }),
    ] {
        let mut value = inbox(transaction(direction));
        assert!(value.state == FundsEvidenceInboxState::Received);
        value.apply(resource.clone()).unwrap();
        assert!(value.state == FundsEvidenceInboxState::Applied);
        assert!(value.applied_resource.as_ref() == Some(&resource));
        let before = value.clone();
        value.apply(resource).unwrap();
        assert!(value == before);
        let restored: FundsEvidenceInbox =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert!(restored == value);
    }
}

#[test]
fn inbox_rejects_wrong_direction_resource_and_review_state_without_mutating_evidence() {
    let mut value = inbox(transaction(FundsTransactionDirection::Debit));
    let before = value.clone();
    assert!(value.apply(FundsAppliedResource::ReturnUsage { usage_id: "return".into() }).is_err());
    assert!(value == before);
    assert!(value.apply(FundsAppliedResource::DebitUsage { usage_id: " ".into() }).is_err());
    assert!(value == before);

    value.apply(FundsAppliedResource::DebitUsage { usage_id: "first-usage".into() }).unwrap();
    let applied = value.clone();
    assert!(value.apply(FundsAppliedResource::DebitUsage { usage_id: "another-usage".into() }).is_err());
    assert!(value == applied);
    assert!(value.apply(FundsAppliedResource::ReturnUsage { usage_id: "first-usage".into() }).is_err());
    assert!(value == applied);

    for mut reviewing in [inbox(unresolved(None)), inbox(transaction(FundsTransactionDirection::Debit))] {
        reviewing.state = FundsEvidenceInboxState::NeedsReview;
        let before = reviewing.clone();
        assert!(reviewing.apply(FundsAppliedResource::DebitUsage { usage_id: "usage".into() }).is_err());
        assert!(reviewing == before);
    }
}

#[test]
fn unknown_zero_and_negative_observations_do_not_become_available_transactions() {
    let unknown = unresolved(None);
    unknown.validate().unwrap();
    assert!(unknown.initial_state() == FundsEvidenceInboxState::NeedsReview);
    assert!(matches!(unknown.semantic_result(), FundsEvidenceResult::Unresolved { amount: None, .. }));
    let explicit_zero = unresolved(Some(Amount::zero()));
    assert!(unknown.semantic_result() != explicit_zero.semantic_result());
    assert!(explicit_zero.initial_state() == FundsEvidenceInboxState::NeedsReview);
    for value in ["0", "-0.01"] {
        let mut input = transaction(FundsTransactionDirection::Debit);
        if let FundsEvidenceResult::Transaction { amount: observed, .. } = &mut input.result {
            *observed = amount(value);
        }
        input.validate().unwrap();
        assert!(input.initial_state() == FundsEvidenceInboxState::NeedsReview);
        let mut received = inbox(input);
        let before = received.clone();
        assert!(received.apply(FundsAppliedResource::DebitUsage { usage_id: "usage".into() }).is_err());
        assert!(received == before);
    }
}

fn window() -> FundsScanWindow {
    FundsScanWindow { since: Instant::from_unix_secs(1), through: Instant::from_unix_secs(100) }
}

fn page(requested: Option<&str>, next: Option<&str>, complete: bool) -> FundsEvidencePage {
    FundsEvidencePage {
        window: window(),
        requested_cursor: requested.map(str::to_string),
        next_cursor: next.map(str::to_string),
        complete,
        observed_at: Instant::from_unix_secs(101),
        source_revision: None,
        observations: vec![],
    }
}

fn checkpoint() -> FundsScanCheckpoint {
    FundsScanCheckpoint {
        base: BaseModel::fake(),
        account_id: "account".into(),
        source_account_reference: "supplier-account".into(),
        scan_key: "scan".into(),
        window: window(),
        cursor: None,
        page_number: 0,
        read_complete: false,
        finished: false,
        completed_through: None,
        pending_inbox_ids: vec![],
        last_page: None,
    }
}

#[test]
fn empty_intermediate_pages_advance_and_restart_keeps_the_exact_window_cursor_and_pending_ids() {
    let first = page(None, Some("opaque-middle"), false);
    first.validate().unwrap();
    assert!(first.observations.is_empty());
    let mut saved = checkpoint();
    saved.advance(&first, "empty-page-digest".into(), vec![]).unwrap();
    assert!(saved.cursor.as_deref() == Some("opaque-middle"));
    assert!(saved.page_number == 1);
    assert!(saved.pending_inbox_ids.is_empty());

    let mut middle = page(Some("opaque-middle"), Some("opaque-next"), false);
    middle.observations = vec![transaction(FundsTransactionDirection::Debit), unresolved(None)];
    let pending = vec!["inbox-one".to_string(), "inbox-two".to_string()];
    saved.advance(&middle, "middle-page-digest".into(), pending.clone()).unwrap();
    assert!(saved.window == window());
    assert!(saved.cursor.as_deref() == Some("opaque-next"));
    assert!(saved.page_number == 2);
    assert!(!saved.read_complete);
    assert!(!saved.finished);
    assert!(saved.pending_inbox_ids == pending);

    let mut recovered: FundsScanCheckpoint =
        serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
    assert!(recovered == saved);
    assert!(recovered.pending_inbox_ids == pending);

    let before = recovered.clone();
    assert!(recovered.advance(&middle, "same-page-digest".into(), vec![]).is_err());
    assert!(recovered == before);

    let mut next = page(Some("opaque-next"), None, true);
    let mut last_observation = transaction(FundsTransactionDirection::Return);
    if let FundsEvidenceResult::Transaction { stable_transaction_id, .. } = &mut last_observation.result {
        *stable_transaction_id = "last-source-transaction".into();
    }
    next.observations.push(last_observation);
    recovered.ensure_page(&next).unwrap();
    // 收件检查点的推进仅保存本页身份，未决证据仍由收件箱独立持久化。
    recovered.advance(&next, "final-page".into(), vec!["final-inbox".into()]).unwrap();
    assert!(recovered.read_complete);
    assert!(!recovered.finished);
    assert!(recovered.completed_through.is_none());
    assert!(recovered.pending_inbox_ids == vec!["final-inbox".to_string()]);
    assert!(recovered.last_page.as_ref().unwrap().requested_cursor.as_deref() == Some("opaque-next"));
    assert!(recovered.ensure_page(&page(None, None, true)).is_err());
}

#[test]
fn page_cursor_completion_and_fixed_window_must_agree_before_any_checkpoint_change() {
    for invalid in [
        page(None, None, false),
        page(None, Some("next"), true),
        page(Some("same"), Some("same"), false),
        page(None, Some(" "), false),
    ] {
        assert!(invalid.validate().is_err());
    }

    let mut saved = checkpoint();
    let before = saved.clone();
    let mut changed_window = page(None, Some("next"), false);
    changed_window.window.through = Instant::from_unix_secs(101);
    assert!(saved.advance(&changed_window, "changed".into(), vec![]).is_err());
    assert!(saved == before);

    assert!(saved.advance(&page(Some("unrequested"), None, true), "wrong-cursor".into(), vec![]).is_err());
    assert!(saved == before);

    saved.page_number = u64::MAX;
    let overflow = saved.clone();
    assert!(saved.advance(&page(None, None, true), "overflow".into(), vec![]).is_err());
    assert!(saved == overflow);
}

#[test]
fn balance_observation_roundtrip_preserves_sampling_without_replacing_the_account_ledger() {
    let mut account = SupplierDepositAccount::new(
        "account".into(),
        DepositAccountIdentity {
            payer_party_id: "payer".into(),
            supplier_id: "supplier".into(),
            external_account_ref: "supplier-account".into(),
            currency: DepositCurrency::CNY,
        },
    )
    .unwrap();
    account.credit(amount("100")).unwrap();
    account.reserve(amount("20")).unwrap();
    let before = account.clone();
    let observation = FundsBalanceObservation {
        base: BaseModel::fake(),
        account_id: account.base.id.clone(),
        observation: FundsBalanceObservationData {
            observation_id: "balance-sample".into(),
            balance: amount("999"),
            currency: DepositCurrency::CNY,
            observed_at: Instant::from_unix_secs(100),
            source_revision: Some("source-sample".into()),
            archive: archive("balance-archive"),
        },
    };
    let json = serde_json::to_value(&observation).unwrap();
    let restored: FundsBalanceObservation = serde_json::from_value(json.clone()).unwrap();
    assert!(restored == observation);
    assert!(restored.observation.balance == amount("999"));
    assert!(restored.observation.observed_at == Instant::from_unix_secs(100));
    assert!(json.get("book_balance").is_none());
    assert!(json.get("reserved_amount").is_none());
    assert!(account == before);
    assert!(account.book_balance == amount("100"));
    assert!(account.available().unwrap() == amount("80"));
}
