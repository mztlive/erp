use super::*;

fn at(value: i64) -> Instant {
    Instant::from_unix_secs(value)
}

fn binding() -> SupplierCredentialBinding {
    SupplierCredentialBinding {
        provider: SupplierCredentialProvider::JdVop,
        account_scope: "configured-account".into(),
        app_key: "configured-app".into(),
    }
}

fn payload() -> SupplierCredentialPayload {
    SupplierCredentialPayload {
        encrypted_payload: "sealed-token-pair".into(),
        payload_fingerprint: "a".repeat(64),
        expires_at: at(1000),
    }
}

fn request(operation: &str, version: Option<u64>, now: i64) -> SupplierCredentialAuthorizationRequest {
    SupplierCredentialAuthorizationRequest {
        expected_version: version,
        claim: SupplierCredentialRefreshClaim {
            owner_id: "authorization-worker".into(),
            operation_id: operation.into(),
            now: at(now),
            lease_seconds: 30,
        },
        new_authorization: version.is_some(),
        actor_id: "operator".into(),
        evidence_reference: "confirmed-authorization".into(),
        account_identity_fingerprint: "b".repeat(64),
    }
}

fn reserved() -> SupplierCredential {
    SupplierCredential::reserve_authorization("credential".into(), binding(), &request("first", None, 100))
        .unwrap()
}

fn authorized() -> SupplierCredential {
    let mut record = reserved();
    record.mark_authorization_sent(at(101)).unwrap();
    record.mark_authorization_exchange_sent(at(102)).unwrap();
    record.complete_authorization(payload(), Some("c".repeat(64)), at(103)).unwrap();
    record
}

#[test]
fn first_authorization_persists_both_boundaries_before_confirming_candidate_and_uid() {
    let mut record = reserved();
    assert!(record.encrypted_payload.is_empty());
    assert!(record.payload_fingerprint.is_empty());
    assert_eq!(record.account_identity_fingerprint, None);
    assert_eq!(record.authorization_candidate_identity_fingerprint, Some("b".repeat(64)));
    assert_eq!(record.refresh.code(), "AUTHORIZATION_CLAIMED");

    let before = record.clone();
    assert!(!record.claim_refresh(1, &request("refresh", Some(1), 101).claim).unwrap());
    assert!(record.import(payload(), 1, at(101)).is_err());
    assert!(record.complete_authorization(payload(), None, at(101)).is_err());
    assert_eq!(record, before);

    record.mark_authorization_sent(at(101)).unwrap();
    assert!(record.complete_authorization(payload(), None, at(102)).is_err());
    record.mark_authorization_exchange_sent(at(102)).unwrap();
    assert!(record.reject_authorization(at(103)).is_err());

    record.complete_authorization(payload(), Some("c".repeat(64)), at(103)).unwrap();
    assert_eq!(record.base.version, 4);
    assert_eq!(record.account_identity_fingerprint, Some("b".repeat(64)));
    assert_eq!(record.authorization_candidate_identity_fingerprint, None);
    assert_eq!(record.authorized_uid_fingerprint, Some("c".repeat(64)));
    assert_eq!(record.last_completed_authorization_operation_id.as_deref(), Some("first"));
    assert_eq!(record.refresh.code(), "READY");
    let event = &record.authorization_history[0];
    assert_eq!(
        (event.sent_at, event.exchange_sent_at, event.completed_at),
        (Some(at(101)), Some(at(102)), Some(at(103)))
    );
}

#[test]
fn only_explicit_pre_exchange_rejection_unlocks_first_candidate_and_same_operation_stays_read_only() {
    let mut record = reserved();
    record.mark_authorization_sent(at(101)).unwrap();
    record.reject_authorization(at(102)).unwrap();
    assert_eq!(record.refresh.code(), "AUTHORIZATION_REJECTED");
    assert_eq!(record.authorization_candidate_identity_fingerprint, None);
    assert!(record.encrypted_payload.is_empty());

    let mut corrected = request("corrected", Some(3), 103);
    corrected.new_authorization = false;
    corrected.account_identity_fingerprint = "d".repeat(64);
    assert!(record.claim_authorization(&corrected).unwrap());
    assert_eq!(record.authorization_candidate_identity_fingerprint, Some("d".repeat(64)));
    assert_eq!(record.authorization_history[0].rejected_at, Some(at(102)));

    let before = record.clone();
    assert!(!record.claim_authorization(&request("first", None, 104)).unwrap());
    assert!(!record.claim_authorization(&corrected).unwrap());
    assert_eq!(record, before);

    let mut conflicting = corrected;
    conflicting.account_identity_fingerprint = "e".repeat(64);
    assert!(record.claim_authorization(&conflicting).is_err());
    assert_eq!(record, before);
}

#[test]
fn unknown_keeps_candidate_and_requires_new_operation_evidence_and_expired_deadline() {
    let mut record = reserved();
    record.mark_authorization_sent(at(101)).unwrap();
    let before = record.clone();
    assert!(record.claim_authorization(&request("recovery", Some(2), 129)).is_err());
    let mut changed = request("recovery", Some(2), 130);
    changed.account_identity_fingerprint = "d".repeat(64);
    assert!(record.claim_authorization(&changed).is_err());
    let mut implicit = request("recovery", Some(2), 130);
    implicit.new_authorization = false;
    assert!(record.claim_authorization(&implicit).is_err());
    let mut no_evidence = request("recovery", Some(2), 130);
    no_evidence.evidence_reference.clear();
    assert!(record.claim_authorization(&no_evidence).is_err());
    assert_eq!(record, before);

    assert!(record.claim_authorization(&request("recovery", Some(2), 130)).unwrap());
    assert_eq!(record.base.version, 3);
    assert_eq!(record.authorization_history[1].previous_operation_id.as_deref(), Some("first"));
    assert!(!record.claim_authorization(&request("first", None, 131)).unwrap());
}

#[test]
fn exchange_unknown_never_releases_identity_or_automatically_retries_after_deadline() {
    let mut record = reserved();
    record.mark_authorization_sent(at(101)).unwrap();
    record.mark_authorization_exchange_sent(at(102)).unwrap();
    let before = record.clone();
    assert!(record.reject_authorization(at(103)).is_err());
    assert!(record.mark_authorization_exchange_sent(at(103)).is_err());
    assert!(record.mark_authorization_sent(at(103)).is_err());
    assert!(!record.claim_authorization(&request("first", None, 500)).unwrap());
    assert!(!record.claim_refresh(3, &request("refresh", Some(3), 500).claim).unwrap());
    assert_eq!(record, before);

    let restored: SupplierCredential =
        serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(restored, before);
    assert!(matches!(
        restored.refresh,
        SupplierCredentialRefreshState::AuthorizationSentUnknown { exchange_sent_at: Some(_), .. }
    ));
}

#[test]
fn confirmed_account_and_uid_cannot_change_and_old_operation_replay_never_reapplies_tokens() {
    let mut record = authorized();
    let mut changed = request("second", Some(4), 104);
    changed.account_identity_fingerprint = "d".repeat(64);
    let before = record.clone();
    assert!(record.claim_authorization(&changed).is_err());
    assert_eq!(record, before);

    assert!(record.claim_authorization(&request("second", Some(4), 104)).unwrap());
    record.mark_authorization_sent(at(105)).unwrap();
    record.mark_authorization_exchange_sent(at(106)).unwrap();
    let before = record.clone();
    assert!(record.complete_authorization(payload(), None, at(107)).is_err());
    assert!(record.complete_authorization(payload(), Some("d".repeat(64)), at(107)).is_err());
    assert_eq!(record, before);

    record.complete_authorization(payload(), Some("c".repeat(64)), at(107)).unwrap();

    let before = record.clone();
    assert!(!record.claim_authorization(&request("first", None, 108)).unwrap());
    assert_eq!(record, before);
    assert_eq!(record.authorization_history.len(), 2);
    assert_eq!(record.last_completed_authorization_operation_id.as_deref(), Some("second"));
}

#[test]
fn legacy_unverified_import_cannot_authorize_and_legacy_cli_cannot_replace_page_identity() {
    let mut imported = SupplierCredential::new("imported".into(), binding(), payload(), at(100)).unwrap();
    let before = imported.clone();
    assert!(imported.claim_authorization(&request("page", Some(1), 101)).is_err());
    assert_eq!(imported, before);

    let mut page = authorized();
    let before = page.clone();
    assert!(page.import(payload(), 4, at(104)).is_err());
    let recovery = SupplierCredentialReauthorization {
        expected_version: 4,
        new_authorization: true,
        operation_id: "legacy-cli".into(),
        actor_id: "operator".into(),
        evidence_reference: "evidence".into(),
    };
    assert!(page.reauthorize(payload(), &recovery, at(1001)).is_err());
    assert_eq!(page, before);
}

#[test]
fn refresh_and_authorization_exclude_each_other_and_only_expired_unsent_claim_can_be_replaced() {
    let mut record = authorized();
    assert!(record.claim_refresh(4, &request("refresh", Some(4), 104).claim).unwrap());
    let before = record.clone();
    assert!(record.claim_authorization(&request("reauthorize", Some(5), 133)).is_err());
    assert_eq!(record, before);

    assert!(record.claim_authorization(&request("reauthorize", Some(5), 134)).unwrap());
    assert_eq!(record.base.version, 6);
    assert!(!record.claim_refresh(6, &request("refresh-again", Some(6), 135).claim).unwrap());
    record.mark_authorization_sent(at(135)).unwrap();
    assert!(!record.claim_refresh(7, &request("refresh-after-timeout", Some(7), 1000).claim).unwrap());
}

#[test]
fn missing_new_fields_decode_as_unverified_legacy_and_invalid_authorization_is_failure_atomic() {
    let imported = SupplierCredential::new("legacy".into(), binding(), payload(), at(100)).unwrap();
    let mut value = serde_json::to_value(&imported).unwrap();
    for name in [
        "account_identity_fingerprint",
        "authorization_candidate_identity_fingerprint",
        "authorized_uid_fingerprint",
        "last_completed_authorization_operation_id",
        "authorization_history",
    ] {
        value.as_object_mut().unwrap().remove(name);
    }
    let restored: SupplierCredential = serde_json::from_value(value).unwrap();
    assert_eq!(restored, imported);

    assert!(
        SupplierCredential::reserve_authorization(
            "credential".into(),
            binding(),
            &request("first", Some(1), 100)
        )
        .is_err()
    );

    let mut record = reserved();
    let before = record.clone();
    assert!(record.mark_authorization_sent(at(130)).is_err());
    assert!(record.mark_authorization_sent(at(99)).is_err());
    assert_eq!(record, before);
}
