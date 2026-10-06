use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::AccountKind;
use erp_core::ids::{PartyId, SupplierAccountId, SupplierCommercialProfileRevisionId};

use super::*;
use crate::{
    InvoiceType, ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountData,
    SupplierAccountStatus, SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData,
};

fn supplier_actor() -> AuditActor {
    AuditActor::new("external-user".into(), "supplier-login".into(), AccountKind::Supplier)
}
fn confirmer() -> AuditActor {
    AuditActor::new("buyer".into(), "procurement".into(), AccountKind::Admin)
}
fn request() -> CooperationRequest {
    CooperationRequest {
        expected_supplier_version: 1,
        expected_profile_id: "profile-1".into(),
        settlement_mode: SettlementMode::PayAfterUse,
        reconciliation_cycle: ReconciliationCycle::Monthly,
        payment_term: "NET-30".into(),
        reason: "希望调整后续付款条件".into(),
    }
}
fn draft() -> CooperationApplication {
    let mut app =
        CooperationApplication::new("application-1".into(), "supplier-1", request(), &supplier_actor())
            .unwrap();
    app.base = BaseModel { id: "application-1".into(), ..BaseModel::fake() };
    app
}
fn submitted() -> CooperationApplication {
    let mut app = draft();
    app.submit("supplier-1", 1, "buyer", "task-1", &supplier_actor(), 100).unwrap();
    app
}
fn result() -> CooperationResult {
    CooperationResult {
        profile_id: "profile-2".into(),
        profile_revision_no: 2,
        supplier_version: 2,
        confirmed_by: "buyer".into(),
        confirmed_at: 200,
    }
}
fn state(status: CooperationStatus) -> CooperationApplication {
    let mut app = if status == CooperationStatus::Draft { draft() } else { submitted() };
    match status {
        CooperationStatus::Returned => {
            app.return_to_supplier(1, &confirmer(), "请补充说明".into(), 150).unwrap()
        },
        CooperationStatus::Withdrawn => app.withdraw("supplier-1", 1, &supplier_actor(), 150).unwrap(),
        CooperationStatus::Effective => app.activate(1, &confirmer(), result()).unwrap(),
        _ => {},
    }
    app
}

#[test]
fn full_state_matrix_separates_edit_submit_and_final_decisions() {
    for status in [
        CooperationStatus::Draft,
        CooperationStatus::Submitted,
        CooperationStatus::Returned,
        CooperationStatus::Withdrawn,
        CooperationStatus::Effective,
    ] {
        let editable = matches!(
            status,
            CooperationStatus::Draft | CooperationStatus::Returned | CooperationStatus::Withdrawn
        );
        let mut app = state(status);
        assert_eq!(app.edit("supplier-1", 1, request(), &supplier_actor()).is_ok(), editable);
        let mut app = state(status);
        assert_eq!(
            app.submit("supplier-1", 1, "buyer", "task-new", &supplier_actor(), 300).is_ok(),
            editable
        );
        let mut app = state(status);
        assert_eq!(
            app.withdraw("supplier-1", 1, &supplier_actor(), 300).is_ok(),
            status == CooperationStatus::Submitted
        );
        let mut app = state(status);
        assert_eq!(
            app.return_to_supplier(1, &confirmer(), "原因".into(), 300).is_ok(),
            status == CooperationStatus::Submitted
        );
        let mut app = state(status);
        assert_eq!(app.activate(1, &confirmer(), result()).is_ok(), status == CooperationStatus::Submitted);
    }
}

#[test]
fn supplier_identity_binding_and_application_version_fail_closed() {
    let mut app = draft();
    assert!(CooperationApplication::new("id".into(), "supplier-1", request(), &confirmer()).is_err());
    assert!(app.edit("supplier-2", 1, request(), &supplier_actor()).is_err());
    assert!(app.edit("supplier-1", 2, request(), &supplier_actor()).is_err());
    assert!(app.edit("supplier-1", 1, request(), &confirmer()).is_err());
    let before = app.clone();
    assert!(app.submit("supplier-1", 1, "", "task", &supplier_actor(), 100).is_err());
    assert_eq!(app, before);
    let mut app = submitted();
    assert!(app.activate(1, &supplier_actor(), result()).is_err());
    assert!(app.return_to_supplier(1, &confirmer(), "   ".into(), 200).is_err());
}

#[test]
fn resubmission_preserves_original_snapshot_and_records_actors_separately() {
    let mut app = submitted();
    let original = app.submissions[0].clone();
    app.return_to_supplier(1, &confirmer(), "请核对".into(), 200).unwrap();
    let returned = app.clone();
    let mut edited = request();
    edited.payment_term = "NET-15".into();
    app.edit("supplier-1", 1, edited, &supplier_actor()).unwrap();
    app.ensure_history_preserved(&returned).unwrap();
    app.submit("supplier-1", 1, "buyer", "task-2", &supplier_actor(), 300).unwrap();
    assert_eq!(app.submissions[0], original);
    assert_eq!(app.submissions[1].submission_no, 2);
    assert_eq!(app.submissions[1].proposal.payment_term, "POSTPAY_NET15");
    assert_eq!(app.submissions[1].submitted_by, "external-user");
    assert_eq!(app.decisions[0].actor_id, "buyer");
    assert_eq!(app.decisions[0].actor_kind, AccountKind::Admin);
    app.activate(1, &confirmer(), result()).unwrap();
    assert_eq!(app.result.as_ref().unwrap().confirmed_by, "buyer");
    assert_eq!(app.created_by, "external-user");
}

#[test]
fn repository_invariant_rejects_rewritten_submission_and_illegal_direct_state_change() {
    let before = submitted();
    let mut changed = before.clone();
    changed.submissions[0].proposal.reason = "伪造".into();
    assert!(changed.ensure_history_preserved(&before).is_err());
    let mut changed = before.clone();
    changed.status = CooperationStatus::Returned;
    assert!(changed.ensure_history_preserved(&before).is_err());
    let before = draft();
    let mut changed = before.clone();
    changed.status = CooperationStatus::Effective;
    changed.result = Some(result());
    assert!(changed.ensure_history_preserved(&before).is_err());
}

#[test]
fn allowlist_rejects_supplier_result_and_internal_party_injection() {
    let value = serde_json::to_value(request()).unwrap();
    for field in ["supplier_id", "confirmed_by", "signing_entity_party_id", "bank_account", "result"] {
        let mut injected = value.clone();
        injected[field] = serde_json::json!("forged");
        assert!(serde_json::from_value::<CooperationRequest>(injected).is_err());
    }
    let app = submitted();
    let view = serde_json::to_value(CooperationView::from_application(&app)).unwrap();
    assert!(view.get("supplier_id").is_none());
    assert!(view.get("signing_entity_party_id").is_none());
    assert!(view["submissions"][0].get("procurement_owner_id").is_none());
    assert!(view["submissions"][0].get("task_id").is_none());
}

#[test]
fn invalid_business_terms_and_empty_reason_are_rejected_before_mutation() {
    let mut value = request();
    value.payment_term = "先用后付".into();
    assert!(value.normalized().is_err());
    let mut value = request();
    value.settlement_mode = SettlementMode::Prepayment;
    assert!(value.normalized().is_err());
    let mut value = request();
    value.reason = " ".into();
    assert!(value.normalized().is_err());
    let mut value = request();
    value.expected_supplier_version = 0;
    assert!(value.normalized().is_err());
    let mut value = request();
    value.payment_term = "PERIOD_MONTH_30".into();
    value.settlement_mode = SettlementMode::Monthly;
    value.reconciliation_cycle = ReconciliationCycle::Weekly;
    assert!(value.normalized().is_err());
}

fn formal_facts() -> (SupplierAccount, SupplierCommercialProfileRevision) {
    let supplier = SupplierAccount::new(
        SupplierAccountId::new("supplier-1"),
        SupplierAccountData {
            party_id: PartyId::new("sensitive-supplier-party"),
            supplier_no: "SUP-1".into(),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: Some(SupplierCommercialProfileRevisionId::new(
                "profile-1",
            )),
            maintainer_user_id: "buyer".into(),
            business_org_unit_id: "org".into(),
            status: SupplierAccountStatus::Active,
        },
        "internal-creator",
    )
    .unwrap();
    let profile = SupplierCommercialProfileRevision::new(
        SupplierCommercialProfileRevisionId::new("profile-1"),
        SupplierCommercialProfileRevisionData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            revision_no: 1,
            settlement_mode: SettlementMode::Prepayment,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: "PREPAY_30".into(),
            business_category: Some("食品".into()),
            invoice_type: InvoiceType::Electronic,
            invoice_tax_rate: None,
            invoice_tax_rates: Some(vec![]),
            signing_entity_party_id: PartyId::new("internal-sign-party"),
            payment_entity_party_id: PartyId::new("internal-pay-party"),
            change_reason: "原档案".into(),
        },
    )
    .unwrap();
    (supplier, profile)
}

#[test]
fn confirmation_creates_profile_without_overwriting_other_commercial_fields() {
    let (supplier, profile) = formal_facts();
    let app = submitted();
    let planned = plan_confirmed_cooperation(
        &app,
        &supplier,
        &profile,
        SupplierCommercialProfileRevisionId::new("profile-2"),
        &confirmer(),
        200,
    )
    .unwrap();
    assert_eq!(planned.profile.payment_term_snapshot, "POSTPAY_NET30");
    assert_eq!(planned.profile.revision.revision_no, 2);
    assert_eq!(planned.profile.business_category, profile.business_category);
    assert_eq!(planned.profile.invoice_type, profile.invoice_type);
    assert_eq!(planned.profile.signing_entity_party_id, profile.signing_entity_party_id);
    assert_eq!(planned.profile.payment_entity_party_id, profile.payment_entity_party_id);
    assert_eq!(supplier.current_commercial_profile_revision_id.unwrap().to_string(), "profile-1");
    assert_eq!(planned.supplier.current_commercial_profile_revision_id.unwrap().to_string(), "profile-2");
    assert_eq!(planned.result.confirmed_by, "buyer");
    assert_eq!(app.submissions[0].submitted_by, "external-user");
}

#[test]
fn confirmation_rejects_current_supplier_or_profile_drift_and_disabled_supplier() {
    let (supplier, profile) = formal_facts();
    let app = submitted();
    let plan = |supplier: &SupplierAccount, profile: &SupplierCommercialProfileRevision| {
        plan_confirmed_cooperation(
            &app,
            supplier,
            profile,
            SupplierCommercialProfileRevisionId::new("profile-2"),
            &confirmer(),
            200,
        )
    };
    let mut changed = supplier.clone();
    changed.base.version = 2;
    assert!(plan(&changed, &profile).is_err());
    let mut changed = supplier.clone();
    changed.current_commercial_profile_revision_id =
        Some(SupplierCommercialProfileRevisionId::new("profile-new"));
    assert!(plan(&changed, &profile).is_err());
    let mut changed = supplier.clone();
    changed.stable.status = SupplierAccountStatus::Disabled;
    assert!(plan(&changed, &profile).is_err());
    let mut changed = profile.clone();
    changed.supplier_id = SupplierAccountId::new("supplier-2");
    assert!(plan(&supplier, &changed).is_err());
}

#[test]
fn command_replay_preserves_original_result_and_detects_payload_identity_conflict() {
    let app = submitted();
    let command = cooperation_command(
        &supplier_actor(),
        "supplier-1",
        "application-1",
        "submit",
        "same-key",
        &request(),
    )
    .unwrap();
    let receipt = CooperationReceipt::new(&command, &app).unwrap();
    receipt.ensure_replayable(&command, "supplier-1").unwrap();
    let mut changed = request();
    changed.reason = "其他请求".into();
    let conflict =
        cooperation_command(&supplier_actor(), "supplier-1", "application-1", "submit", "same-key", &changed)
            .unwrap();
    assert_eq!(command.id(), conflict.id());
    assert!(receipt.ensure_replayable(&conflict, "supplier-1").is_err());
    assert!(receipt.ensure_replayable(&command, "supplier-2").is_err());
    let stored = serde_json::to_string(&receipt).unwrap();
    assert!(!stored.contains("same-key"));
    assert_eq!(receipt.status, CooperationStatus::Submitted);
}

#[test]
fn task_handover_can_change_confirmer_without_rewriting_frozen_original_owner() {
    let mut app = submitted();
    let reassigned = AuditActor::new("new-owner".into(), "new-buyer".into(), AccountKind::Admin);
    let mut confirmed = result();
    confirmed.confirmed_by = reassigned.id().into();
    app.activate(1, &reassigned, confirmed).unwrap();
    assert_eq!(app.submissions[0].procurement_owner_id, "buyer");
    assert_eq!(app.decisions[0].actor_id, "new-owner");
}
