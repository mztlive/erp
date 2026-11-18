use mongodb::bson::Bson;

use super::*;
use crate::entity::supplier_credential::{
    SupplierCredentialPayload, SupplierCredentialProvider, SupplierCredentialReauthorization,
};

fn at(value: i64) -> Instant {
    Instant::from_unix_secs(value)
}

fn record() -> SupplierCredential {
    SupplierCredential::new(
        "credential".into(),
        SupplierCredentialBinding {
            provider: SupplierCredentialProvider::JdVop,
            account_scope: "account".into(),
            app_key: "app".into(),
        },
        payload(),
        at(100),
    )
    .unwrap()
}

fn payload() -> SupplierCredentialPayload {
    SupplierCredentialPayload {
        encrypted_payload: "$sealed-value".into(),
        payload_fingerprint: "a".repeat(64),
        expires_at: at(1000),
    }
}

fn claimed_record() -> SupplierCredential {
    let mut record = record();
    record
        .claim_refresh(
            1,
            &SupplierCredentialRefreshClaim {
                owner_id: "owner".into(),
                operation_id: "operation".into(),
                now: at(101),
                lease_seconds: 30,
            },
        )
        .unwrap();
    record
}

#[test]
fn every_cas_filter_carries_original_identity_and_numeric_expected_version() {
    let mut sent_record = claimed_record();
    sent_record.mark_refresh_sent(at(102)).unwrap();
    for (filter, expected) in [
        (ordinary_import(&record(), 1, at(101)).unwrap(), 1),
        (claimed(&claimed_record(), at(102)).unwrap(), 2),
        (sent(&sent_record).unwrap(), 3),
        (reauthorization(&sent_record, 3, at(131)).unwrap(), 3),
    ] {
        assert_eq!(filter.get_str("id").unwrap(), "credential");
        assert_eq!(filter.get_str("provider").unwrap(), "jd_vop");
        assert_eq!(filter.get_str("account_scope").unwrap(), "account");
        assert_eq!(filter.get_str("app_key").unwrap(), "app");
        assert_eq!(filter.get_i64("version").unwrap(), expected);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
    }
}

#[test]
fn claim_and_import_only_admit_ready_or_expired_unsent_claims() {
    let original = record();
    let command = SupplierCredentialRefreshClaim {
        owner_id: "owner".into(),
        operation_id: "operation".into(),
        now: at(101),
        lease_seconds: 30,
    };
    let filter = claim(&original, 1, &command).unwrap();
    assert_eq!(
        filter.get_array("$or").unwrap(),
        &vec![
            doc! {"refresh.state":"READY"}.into(),
            doc! {"refresh.state":"CLAIMED","refresh.lease_until":{"$lte":101_i64}}.into(),
        ]
    );
    assert_eq!(
        filter.get_document("last_completed_refresh_operation_id").unwrap(),
        &doc! {"$ne":"operation"}
    );
    assert!(ordinary_import(&original, u64::MAX, at(101)).is_err());
}

#[test]
fn sending_requires_live_lease_but_original_late_success_keeps_owner_and_operation_fence() {
    let mut record = claimed_record();
    let before = claimed(&record, at(102)).unwrap();
    assert_eq!(before.get_document("refresh.lease_until").unwrap(), &doc! {"$gt":102_i64});
    assert_eq!(before.get_str("refresh.owner_id").unwrap(), "owner");
    assert_eq!(before.get_str("refresh.operation_id").unwrap(), "operation");

    record.mark_refresh_sent(at(102)).unwrap();
    let completion = sent(&record).unwrap();
    assert_eq!(completion.get_str("refresh.state").unwrap(), "SENT_UNKNOWN");
    assert_eq!(completion.get_str("refresh.owner_id").unwrap(), "owner");
    assert_eq!(completion.get_str("refresh.operation_id").unwrap(), "operation");
    assert_eq!(completion.get_i64("refresh.sent_at").unwrap(), 102);
    assert!(!completion.contains_key("refresh.lease_until"));

    record
        .reauthorize(
            payload(),
            &SupplierCredentialReauthorization {
                expected_version: 3,
                new_authorization: true,
                operation_id: "new-authorization".into(),
                actor_id: "operator".into(),
                evidence_reference: "approval".into(),
            },
            at(131),
        )
        .unwrap();
    assert_ne!(completion.get_i64("version").unwrap(), i64::try_from(record.base.version).unwrap());
    assert_eq!(record.refresh.code(), "READY");
}

#[test]
fn recovery_deadline_is_in_atomic_filter_and_replacement_treats_ciphertext_as_literal() {
    let mut record = claimed_record();
    record.mark_refresh_sent(at(102)).unwrap();
    let filter = reauthorization(&record, 3, at(131)).unwrap();
    assert_eq!(
        filter.get_array("$or").unwrap(),
        &vec![
            doc! {"refresh.state":"SENT_UNKNOWN","refresh.lease_until":{"$lte":131_i64}}.into(),
            doc! {"refresh.state":"REAUTHORIZATION_REQUIRED"}.into(),
            doc! {"refresh.state":{"$in":["READY","CLAIMED"]},"expires_at":{"$lte":131_i64}}.into(),
        ]
    );

    let pipeline = replacement(&record).unwrap();
    let fields = pipeline[0].get_document("$set").unwrap();
    assert_eq!(
        fields.get_document("encrypted_payload").unwrap().get_str("$literal").unwrap(),
        "$sealed-value"
    );
    assert_eq!(fields.get_document("version").unwrap().get_i64("$literal").unwrap(), 3);
}

fn authorization_request(
    operation: &str,
    version: Option<u64>,
    now: i64,
) -> SupplierCredentialAuthorizationRequest {
    SupplierCredentialAuthorizationRequest {
        expected_version: version,
        claim: SupplierCredentialRefreshClaim {
            owner_id: "owner".into(),
            operation_id: operation.into(),
            now: at(now),
            lease_seconds: 30,
        },
        new_authorization: version.is_some(),
        actor_id: "operator".into(),
        evidence_reference: "verified-authorization".into(),
        account_identity_fingerprint: "b".repeat(64),
    }
}

fn reserved_authorization() -> SupplierCredential {
    SupplierCredential::reserve_authorization(
        "credential".into(),
        record().binding(),
        &authorization_request("authorize", None, 100),
    )
    .unwrap()
}

#[test]
fn authorization_boundary_filters_fence_owner_operation_version_and_exchange_phase() {
    let mut record = reserved_authorization();
    let claim = authorization_claimed(&record, at(101)).unwrap();
    assert_eq!(claim.get_i64("version").unwrap(), 1);
    assert_eq!(claim.get_str("refresh.owner_id").unwrap(), "owner");
    assert_eq!(claim.get_str("refresh.operation_id").unwrap(), "authorize");
    assert_eq!(claim.get_str("refresh.state").unwrap(), "AUTHORIZATION_CLAIMED");
    assert_eq!(claim.get_document("refresh.lease_until").unwrap(), &doc! {"$gt":101_i64});

    record.mark_authorization_sent(at(101)).unwrap();
    let sending = authorization_sent(&record).unwrap();
    assert_eq!(sending.get_i64("version").unwrap(), 2);
    assert_eq!(sending.get_i64("refresh.sent_at").unwrap(), 101);
    assert_eq!(sending.get("refresh.exchange_sent_at"), Some(&Bson::Null));
    let exchange = authorization_exchange(&record, at(102)).unwrap();
    assert_eq!(exchange.get_document("refresh.lease_until").unwrap(), &doc! {"$gt":102_i64});

    record.mark_authorization_exchange_sent(at(102)).unwrap();
    let completion = authorization_sent(&record).unwrap();
    assert_eq!(completion.get_i64("version").unwrap(), 3);
    assert_eq!(completion.get_i64("refresh.exchange_sent_at").unwrap(), 102);
    assert_eq!(completion.get_str("refresh.owner_id").unwrap(), "owner");
    assert_eq!(completion.get_str("refresh.operation_id").unwrap(), "authorize");
    assert!(!completion.contains_key("refresh.lease_until"));

    record.complete_authorization(payload(), None, at(150)).unwrap();
    assert_ne!(completion.get_i64("version").unwrap(), i64::try_from(record.base.version).unwrap());
    assert_eq!(record.refresh.code(), "READY");
}

#[test]
fn authorization_recovery_cas_guards_original_identity_deadline_and_historical_operation() {
    let mut record = reserved_authorization();
    record.mark_authorization_sent(at(101)).unwrap();
    let request = authorization_request("recover-authorization", Some(2), 130);
    let filter = authorization_claim(&record, &request).unwrap();
    assert_eq!(filter.get_str("id").unwrap(), "credential");
    assert_eq!(filter.get_str("provider").unwrap(), "jd_vop");
    assert_eq!(filter.get_str("account_scope").unwrap(), "account");
    assert_eq!(filter.get_str("app_key").unwrap(), "app");
    assert_eq!(filter.get_i64("version").unwrap(), 2);
    assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
    assert_eq!(filter.get("account_identity_fingerprint"), Some(&Bson::Null));
    assert_eq!(filter.get_str("authorization_candidate_identity_fingerprint").unwrap(), "b".repeat(64));
    assert_eq!(
        filter.get_document("authorization_history.operation_id").unwrap(),
        &doc! {"$ne":"recover-authorization"}
    );
    let expiry = filter.get_array("$or").unwrap()[1].as_document().unwrap();
    assert_eq!(expiry.get_document("refresh.lease_until").unwrap(), &doc! {"$lte":130_i64});

    assert!(record.claim_authorization(&request).unwrap());
    assert_eq!(record.base.version, 3);
    assert_ne!(authorization_sent(&reserved_sent_authorization()).unwrap().get_i64("version").unwrap(), 3);
}

fn reserved_sent_authorization() -> SupplierCredential {
    let mut record = reserved_authorization();
    record.mark_authorization_sent(at(101)).unwrap();
    record
}
