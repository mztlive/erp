use super::*;

fn at(value: i64) -> Instant {
    Instant::from_unix_secs(value)
}

fn binding() -> SupplierCredentialBinding {
    SupplierCredentialBinding {
        provider: SupplierCredentialProvider::JdVop,
        account_scope: "account-scope".into(),
        app_key: "application-key".into(),
    }
}

fn payload(cipher: &str) -> SupplierCredentialPayload {
    SupplierCredentialPayload {
        encrypted_payload: cipher.into(),
        payload_fingerprint: "a".repeat(64),
        expires_at: at(1000),
    }
}

fn record() -> SupplierCredential {
    let mut record =
        SupplierCredential::new("credential-id".into(), binding(), payload("sealed-original"), at(100))
            .unwrap();
    record.base =
        BaseModel { id: "credential-id".into(), created_at: 100, updated_at: 100, ..BaseModel::fake() };
    record
}

fn claim(owner: &str, operation: &str, now: i64) -> SupplierCredentialRefreshClaim {
    SupplierCredentialRefreshClaim {
        owner_id: owner.into(),
        operation_id: operation.into(),
        now: at(now),
        lease_seconds: 30,
    }
}

fn sent() -> SupplierCredential {
    let mut record = record();
    record.claim_refresh(1, &claim("worker-one", "refresh-one", 101)).unwrap();
    record.mark_refresh_sent(at(102)).unwrap();
    record
}

fn recovery(version: u64) -> SupplierCredentialReauthorization {
    SupplierCredentialReauthorization {
        expected_version: version,
        new_authorization: true,
        operation_id: "new-authorization-one".into(),
        actor_id: "operator".into(),
        evidence_reference: "authorization-record".into(),
    }
}

#[test]
fn ordinary_import_requires_original_identity_and_version_and_redacts_every_sensitive_debug_view() {
    let mut record = record();
    for changed in [
        SupplierCredentialBinding { account_scope: "other".into(), ..binding() },
        SupplierCredentialBinding { app_key: "other".into(), ..binding() },
    ] {
        assert!(record.require_binding(&changed).is_err());
    }

    let before = record.clone();
    assert!(record.import(payload("sealed-new"), 2, at(101)).is_err());
    assert_eq!(record, before);
    record.import(payload("sealed-new"), 1, at(101)).unwrap();
    assert_eq!(record.base.version, 2);
    assert_eq!(record.encrypted_payload, "sealed-new");

    for debug in [format!("{record:?}"), format!("{:?}", binding()), format!("{:?}", payload("secret"))] {
        for forbidden in ["sealed-new", "application-key", "account-scope", "secret"] {
            assert!(!debug.contains(forbidden));
        }
    }
}

#[test]
fn unexpired_claim_has_one_owner_and_only_unsent_expired_claim_can_be_replaced() {
    let mut record = record();
    assert!(record.claim_refresh(1, &claim("worker-one", "refresh-one", 101)).unwrap());
    let old = record.clone();
    assert!(!record.claim_refresh(2, &claim("worker-two", "refresh-two", 102)).unwrap());
    assert_eq!(record, old);

    let mut expired_old = old;
    assert!(expired_old.mark_refresh_sent(at(131)).is_err());
    assert!(record.claim_refresh(2, &claim("worker-two", "refresh-two", 131)).unwrap());
    assert_eq!(record.base.version, 3);
    assert!(matches!(&record.refresh, SupplierCredentialRefreshState::Claimed { owner_id, operation_id, .. }
        if owner_id == "worker-two" && operation_id == "refresh-two"));
}

#[test]
fn sent_unknown_survives_timeout_crash_and_import_and_never_returns_to_ready_by_lease_expiry() {
    let mut record = sent();
    let before = record.clone();
    assert!(!record.claim_refresh(3, &claim("new-worker", "new-operation", 500)).unwrap());
    assert!(record.import(payload("new-cipher"), 3, at(500)).is_err());
    assert!(record.release_claim(at(103)).is_err());
    assert!(record.mark_refresh_sent(at(103)).is_err());
    assert_eq!(record, before);

    let restored: SupplierCredential =
        serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(restored.refresh.code(), "SENT_UNKNOWN");
    assert_eq!(restored.refresh.deadline(), Some(at(131)));
    assert_eq!(restored, before);
}

#[test]
fn original_late_success_keeps_one_completion_receipt_and_rejects_expired_result_without_mutation() {
    let mut record = sent();
    let before = record.clone();
    assert!(
        record
            .complete_refresh(
                SupplierCredentialPayload { expires_at: at(140), ..payload("expired") },
                at(150)
            )
            .is_err()
    );
    assert_eq!(record, before);

    record.complete_refresh(payload("sealed-rotated"), at(150)).unwrap();
    assert_eq!(record.refresh.code(), "READY");
    assert_eq!(record.last_completed_refresh_operation_id.as_deref(), Some("refresh-one"));
    assert_eq!(record.base.version, 4);

    let version = record.base.version;
    assert!(record.claim_refresh(version, &claim("worker", "refresh-one", 151)).is_err());
    assert!(record.complete_refresh(payload("duplicate-result"), at(151)).is_err());
    assert_eq!(record.encrypted_payload, "sealed-rotated");
}

#[test]
fn reauthorization_requires_expired_sending_deadline_and_preserves_original_operation_and_actor() {
    let mut record = sent();
    let before = record.clone();
    assert!(record.reauthorize(payload("new-authorization"), &recovery(3), at(130)).is_err());
    assert_eq!(record, before);
    let mut implicit = recovery(3);
    implicit.new_authorization = false;
    assert!(record.reauthorize(payload("new-authorization"), &implicit, at(131)).is_err());
    assert!(record.reauthorize(payload("new-authorization"), &recovery(2), at(131)).is_err());

    assert!(record.reauthorize(payload("new-authorization"), &recovery(3), at(131)).unwrap());
    assert_eq!(record.base.id, "credential-id");
    assert_eq!(record.binding(), binding());
    assert_eq!(record.base.version, 4);
    assert_eq!(record.reauthorization_history.len(), 1);
    let event = &record.reauthorization_history[0];
    assert_eq!(event.actor_id, "operator");
    assert_eq!(event.previous_refresh_operation_id.as_deref(), Some("refresh-one"));
    assert_eq!(event.previous_version, 3);
    assert_eq!(event.evidence_reference, "authorization-record");
}

#[test]
fn same_semantic_reauthorization_replays_across_random_ciphertexts_but_different_payloads_are_rejected() {
    let mut record = sent();
    record.reauthorize(payload("cipher-nonce-one"), &recovery(3), at(131)).unwrap();

    let before = record.clone();
    assert!(!record.reauthorize(payload("cipher-nonce-two"), &recovery(3), at(132)).unwrap());
    assert!(!record.reauthorize(payload("cipher-nonce-three"), &recovery(3), at(2000)).unwrap());
    assert_eq!(record, before);

    let changed =
        SupplierCredentialPayload { payload_fingerprint: "b".repeat(64), ..payload("cipher-other") };
    assert!(record.reauthorize(changed, &recovery(3), at(132)).is_err());
    let mut other_actor = recovery(3);
    other_actor.actor_id = "other-operator".into();
    assert!(record.reauthorize(payload("cipher-other"), &other_actor, at(132)).is_err());
    assert_eq!(record, before);
}

#[test]
fn verified_invalid_grant_requires_new_authorization_and_does_not_restore_old_token() {
    let mut record = sent();
    record.invalidate_refresh(at(103)).unwrap();
    assert_eq!(record.refresh.code(), "REAUTHORIZATION_REQUIRED");
    assert_eq!(record.encrypted_payload, "sealed-original");
    assert!(record.import(payload("ordinary-import"), 4, at(104)).is_err());
    assert!(!record.claim_refresh(4, &claim("worker", "operation", 104)).unwrap());

    assert!(record.reauthorize(payload("new-authorization"), &recovery(4), at(104)).unwrap());
    assert_eq!(record.refresh.code(), "READY");
    assert_eq!(record.base.version, 5);
}

#[test]
fn expired_ready_or_unsent_claim_requires_explicit_new_authorization_but_never_bypasses_sent_deadline() {
    let new_authorization =
        || SupplierCredentialPayload { expires_at: at(2000), ..payload("new-authorization") };

    let mut ready = record();
    let before = ready.clone();
    assert!(ready.reauthorize(new_authorization(), &recovery(1), at(999)).is_err());
    assert_eq!(ready, before);
    assert!(ready.reauthorize(new_authorization(), &recovery(1), at(1000)).unwrap());
    assert_eq!(ready.base.version, 2);
    assert_eq!(ready.reauthorization_history[0].previous_refresh_operation_id, None);
    assert_eq!(ready.reauthorization_history[0].previous_version, 1);

    let mut claimed = record();
    claimed.claim_refresh(1, &claim("worker", "unsent-operation", 999)).unwrap();
    assert!(claimed.reauthorize(new_authorization(), &recovery(2), at(1000)).unwrap());
    assert_eq!(claimed.base.version, 3);
    assert_eq!(
        claimed.reauthorization_history[0].previous_refresh_operation_id.as_deref(),
        Some("unsent-operation")
    );
    assert!(claimed.mark_refresh_sent(at(1000)).is_err());

    let mut unknown = record();
    unknown.claim_refresh(1, &claim("worker", "sent-operation", 999)).unwrap();
    unknown.mark_refresh_sent(at(999)).unwrap();
    let before = unknown.clone();
    assert!(unknown.reauthorize(new_authorization(), &recovery(3), at(1000)).is_err());
    assert_eq!(unknown, before);
    assert!(unknown.reauthorize(new_authorization(), &recovery(3), at(1029)).unwrap());
}

#[test]
fn invalid_metadata_time_and_payload_fail_without_partial_state_change() {
    let mut record = record();
    for (version, lease) in [(1, 0), (1, 301), (2, 30)] {
        let before = record.clone();
        assert!(
            record
                .claim_refresh(
                    version,
                    &SupplierCredentialRefreshClaim {
                        lease_seconds: lease,
                        ..claim("worker", "operation", 101)
                    }
                )
                .is_err()
        );
        assert_eq!(record, before);
    }

    record.base.version = u64::MAX;
    assert!(record.import(payload("new"), u64::MAX, at(101)).is_err());

    for invalid in [
        SupplierCredentialPayload { encrypted_payload: String::new(), ..payload("x") },
        SupplierCredentialPayload { payload_fingerprint: "not-hmac".into(), ..payload("x") },
    ] {
        assert!(invalid.validate(at(100)).is_err());
    }
}
