use application_core::{AuditActor, CommandReceipt};
use erp_core::AccountKind;
use erp_core::common::time::{BusinessDate, Instant};
use serde_json::json;

use super::*;
use crate::dto::supplier_offering::SupplierOfferingTermsWrite;
use crate::entity::supplier_offering::{
    AvailabilityStatus, OfferingSourceType, OfferingStatus, SupplierOffering,
};

fn supplier() -> AuditActor {
    AuditActor::new("portal-user".into(), "supplier-person".into(), AccountKind::Supplier)
}
fn internal() -> AuditActor {
    AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin)
}
fn terms() -> SupplierOfferingTermsWrite {
    serde_json::from_value(json!({"dropship_supply_price_gross":"11.30","bulk_supply_price_gross":"9.04","input_tax_rate":"0.13","bulk_minimum_order_quantity":"10","supply_region":["CN"],"valid_from":"2026-01-01","valid_to":"2026-12-31"})).unwrap()
}
fn snapshot() -> OfferingApplicationSnapshot {
    OfferingApplicationSnapshot::ExistingQuote {
        sku_id: "sku-1".into(),
        target_version: Box::new(QuoteTargetVersion {
            sku_version: 1,
            sku_revision_id: "sku-revision-1".into(),
            sku_revision_version: 1,
            product_id: "product-1".into(),
            product_version: 1,
            product_revision_id: "product-revision-1".into(),
            product_revision_version: 1,
            unit_id: "unit-1".into(),
            unit_version: 1,
        }),
        supplier_sku_code: "supplier-code".into(),
        supplier_product_code: None,
        terms: terms(),
        availability_status: PortalAvailabilityStatus::Available,
        available_quantity: None,
        availability_reported_at: Instant::from_unix_secs(100),
    }
}

#[test]
fn first_quote_requires_the_supplier_observed_target_and_rejects_forged_extra_fields() {
    let original = serde_json::to_value(snapshot()).unwrap();
    serde_json::from_value::<OfferingApplicationSnapshot>(original.clone()).unwrap();
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().remove("target_version");
    assert!(serde_json::from_value::<OfferingApplicationSnapshot>(missing).is_err());
    let mut injected = original;
    injected["target_version"]["internal_cost"] = json!("1.00");
    assert!(serde_json::from_value::<OfferingApplicationSnapshot>(injected).is_err());
}
fn application() -> OfferingApplication {
    OfferingApplication::new("application-1".into(), "supplier-1", &supplier(), snapshot(), "首次报价")
        .unwrap()
}
fn result() -> OfferingApplicationResult {
    OfferingApplicationResult {
        offering_id: "offering-1".into(),
        revision_id: "revision-1".into(),
        revision_no: 1,
        offering_version: 1,
        operation: "CREATED".into(),
    }
}
fn submitted() -> OfferingApplication {
    let mut app = application();
    app.submit(&supplier(), "buyer", "task-1", 1, Instant::from_unix_secs(200)).unwrap();
    app
}

#[test]
fn only_real_supplier_identity_can_create_or_access_portal_draft() {
    assert!(OfferingApplication::new("app".into(), "supplier-1", &internal(), snapshot(), "原因").is_err());
    let app = application();
    app.ensure_owned("supplier-1", &supplier(), app.base.version).unwrap();
    assert!(matches!(
        app.ensure_owned("supplier-2", &supplier(), app.base.version),
        Err(crate::Error::NotFound(_))
    ));
    assert!(app.ensure_owned("supplier-1", &supplier(), app.base.version + 1).is_err());
}
#[test]
fn submitted_snapshot_is_frozen_and_withdrawal_preserves_history() {
    let mut app = submitted();
    let original = serde_json::to_value(&app.submissions).unwrap();
    assert!(app.edit(snapshot(), "改价").is_err());
    app.withdraw(&supplier()).unwrap();
    assert_eq!(app.status, ApplicationStatus::Withdrawn);
    assert!(app.result.is_none());
    assert_eq!(serde_json::to_value(&app.submissions).unwrap(), original);
    assert!(app.decide(&internal(), Some(result()), "通过", Instant::from_unix_secs(300)).is_err());
}
#[test]
fn effective_application_preserves_submitter_and_separate_internal_confirmer() {
    let mut app = submitted();
    app.decision_submission(&internal(), app.base.version, "task-1", 1).unwrap();
    app.decide(&internal(), Some(result()), "通过", Instant::from_unix_secs(300)).unwrap();
    assert_eq!(app.status, ApplicationStatus::Effective);
    assert_eq!(app.submissions[0].submitted_by, "portal-user");
    assert_eq!(app.decisions[0].decided_by, "buyer");
    assert!(app.withdraw(&supplier()).is_err());
    assert!(app.edit(snapshot(), "修改").is_err());
    assert!(app.submit(&supplier(), "buyer", "task-2", 1, Instant::from_unix_secs(400)).is_err());
}
#[test]
fn returned_and_resubmitted_application_append_history_without_rewriting_original() {
    let mut app = submitted();
    let original = serde_json::to_value(&app.submissions[0]).unwrap();
    assert!(app.decide(&internal(), None, " ", Instant::from_unix_secs(300)).is_err());
    app.decide(&internal(), None, "请核对报价", Instant::from_unix_secs(300)).unwrap();
    let before = app.clone();
    app.edit(snapshot(), "已核对报价").unwrap();
    app.submit(&supplier(), "buyer", "task-2", 1, Instant::from_unix_secs(400)).unwrap();
    app.ensure_history_preserved(&before).unwrap();
    assert_eq!(app.submissions.len(), 2);
    assert_eq!(serde_json::to_value(&app.submissions[0]).unwrap(), original);
}
#[test]
fn history_prefix_guard_rejects_changed_or_removed_submissions() {
    let before = submitted();
    let mut changed = before.clone();
    changed.submissions[0].reason = "篡改".into();
    assert!(changed.ensure_history_preserved(&before).is_err());
    changed = before.clone();
    changed.submissions.clear();
    assert!(changed.ensure_history_preserved(&before).is_err());
    changed = before.clone();
    changed.supplier_id = "supplier-2".into();
    assert!(changed.ensure_history_preserved(&before).is_err());
}
#[test]
fn valid_transfer_keeps_original_handler_fact_and_accepts_current_task_version() {
    let app = submitted();
    let actor = AuditActor::new("new-buyer".into(), "buyer-2".into(), AccountKind::Admin);
    app.decision_submission(&actor, app.base.version, "task-1", 3).unwrap();
    assert_eq!(app.submissions[0].handler_id, "buyer");
    assert!(app.decision_submission(&actor, app.base.version, "task-other", 3).is_err());
}
#[test]
fn portal_allow_lists_reject_relation_price_and_server_state_in_availability() {
    let base = json!({"expected_version":1,"availability_status":"AVAILABLE","available_quantity":null,"reason":"补报","idempotency_key":"cmd"});
    serde_json::from_value::<PortalAvailabilityInput>(base.clone()).unwrap();
    for key in ["supplier_id", "terms", "status", "source_updated_at"] {
        let mut malicious = base.clone();
        malicious[key] = json!("value");
        assert!(serde_json::from_value::<PortalAvailabilityInput>(malicious).is_err());
    }
    let mut missing = base.clone();
    missing.as_object_mut().unwrap().remove("expected_version");
    assert!(serde_json::from_value::<PortalAvailabilityInput>(missing).is_err());
    for state in ["STOPPED", "STALE", "UNAVAILABLE"] {
        let mut malicious = base.clone();
        malicious["availability_status"] = json!(state);
        assert!(serde_json::from_value::<PortalAvailabilityInput>(malicious).is_err());
    }
    assert_eq!(
        AvailabilityStatus::from(PortalAvailabilityStatus::OutOfStock),
        AvailabilityStatus::Unavailable
    );
}
#[test]
fn nested_terms_allow_list_and_current_validity_fail_closed() {
    let on = BusinessDate::from_ymd(2026, 10, 4).unwrap();
    validate_portal_terms(&terms(), on).unwrap();
    let mut nested = serde_json::to_value(terms()).unwrap();
    nested["payment_term"] = json!("PREPAY_100");
    assert!(serde_json::from_value::<SupplierOfferingTermsWrite>(nested).is_err());
    let mut future = terms();
    future.valid_from = "2027-01-01".into();
    future.valid_to = None;
    assert!(validate_portal_terms(&future, on).is_err());
    let mut expired = terms();
    expired.valid_to = Some("2026-09-01".into());
    assert!(validate_portal_terms(&expired, on).is_err());
    let mut invalid = terms();
    invalid.input_tax_rate = "bad".into();
    assert!(validate_portal_terms(&invalid, on).is_err());
}
#[test]
fn api_source_is_read_only_without_changing_internal_pause_or_provenance() {
    let mut offering:SupplierOffering=serde_json::from_value(json!({"id":"offering","created_at":1,"updated_at":1,"deleted_at":0,"version":1,"status":"PAUSED","current_revision_id":null,"created_by":"original","updated_by":"original","sku_id":"sku","supplier_id":"supplier","supplier_sku_code":"code","source_type":"EXCEL","maintainer_user_id":"buyer","business_org_unit_id":"org"})).unwrap();
    PortalOfferingService::ensure_portal_writable(&offering).unwrap();
    assert_eq!(offering.stable.status, OfferingStatus::Paused);
    assert_eq!(offering.source_type, OfferingSourceType::Excel);
    offering.source_type = OfferingSourceType::Api;
    assert!(PortalOfferingService::ensure_portal_writable(&offering).is_err());
}
fn command(payload: &serde_json::Value) -> CommandReceipt {
    CommandReceipt::from_payload("portal-", "portal-user", "save", "application", "same-key", payload)
        .unwrap()
}
#[test]
fn independent_receipt_replays_original_and_rejects_changed_payload_or_corruption() {
    let cmd = command(&json!({"price":"100"}));
    let response = json!({"id":"application-1","status":"SUBMITTED"});
    let receipt = PortalCommandReceipt::new(&cmd, "supplier-1", &response).unwrap();
    assert_eq!(receipt.replay(&cmd).unwrap(), response);
    assert!(receipt.replay(&command(&json!({"price":"105"}))).is_err());
    let mut corrupted = receipt.clone();
    corrupted.schema_version = 2;
    assert!(corrupted.replay(&cmd).is_err());
    corrupted = receipt.clone();
    corrupted.base.deleted_at = 1;
    assert!(corrupted.replay(&cmd).is_err());
    corrupted = receipt;
    corrupted.actor_id = "other".into();
    assert!(corrupted.replay(&cmd).is_err());
}
#[test]
fn packaging_price_and_base_unit_quantity_precision_reject_silent_rounding() {
    validate_portal_packaging_price("12.3456").unwrap();
    for input in ["-1", "bad", "1.12345", ""] {
        assert!(validate_portal_packaging_price(input).is_err());
    }
    validate_portal_quantities(&terms(), Some("12"), 0).unwrap();
    assert!(validate_portal_quantities(&terms(), Some("12.5"), 0).is_err());
    assert!(validate_portal_quantities(&terms(), Some("-1"), 6).is_err());
    let mut fractional = terms();
    fractional.bulk_minimum_order_quantity = "1.5".into();
    assert!(validate_portal_quantities(&fractional, None, 0).is_err());
    validate_portal_quantities(&fractional, Some("1.25"), 2).unwrap();
    assert!(validate_portal_quantities(&fractional, None, 7).is_err());
}
#[test]
fn availability_audit_fact_preserves_blank_and_zero_as_separate_facts() {
    use crate::entity::supplier_offering::SupplierOfferingAvailability;
    let base = json!({"id":"availability","created_at":1,"updated_at":1,"deleted_at":0,"version":1,"supplier_offering_id":"offering","availability_status":"AVAILABLE","available_quantity":null,"source_updated_at":100,"received_at":100,"source_revision_token":null,"updated_by":"portal-user"});
    let blank: SupplierOfferingAvailability = serde_json::from_value(base.clone()).unwrap();
    let mut zero_json = base;
    zero_json["available_quantity"] = json!("0");
    let zero: SupplierOfferingAvailability = serde_json::from_value(zero_json).unwrap();
    assert_eq!(PortalAvailabilityFact::from_availability(&blank).available_quantity, None);
    assert_eq!(PortalAvailabilityFact::from_availability(&zero).available_quantity, Some("0".into()));
    assert!(blank.is_available());
    assert!(!zero.is_available());
}
#[test]
fn reported_time_preserves_old_actual_report_and_rejects_future_without_age_policy() {
    let now = Instant::from_unix_secs(1_000);
    validate_portal_reported_at(Instant::from_unix_secs(1), now).unwrap();
    validate_portal_reported_at(now, now).unwrap();
    assert!(validate_portal_reported_at(Instant::from_unix_secs(1_001), now).is_err());
    assert!(validate_portal_reported_at(Instant::from_unix_secs(-1), now).is_err());
}
