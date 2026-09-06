use entities::{
    common::time::BusinessDate,
    supplier::{SupplierProfileCommand, SupplierProfileCommandData},
};

use super::{command_view, SaveSupplierProfileRequest};

#[test]
fn command_replay_is_bound_to_supplier_and_fingerprint_stable() {
    const FP1: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const FP1_V1: &str = "sha256-v1:0000000000000000000000000000000000000000000000000000000000000000";
    const FP2: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    let command = SupplierProfileCommand::new(
        "command-1",
        SupplierProfileCommandData {
            idempotency_key: "key-1".to_string(),
            operation: "update".to_string(),
            request_fingerprint: FP1.to_string(),
            supplier_id: "supplier-1".to_string(),
            supplier_no: "SUP-1".to_string(),
            revision_id: "revision-1".to_string(),
            revision_no: 2,
            supplier_version: 3,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            change_reason: "修订".to_string(),
        },
    )
    .unwrap();
    assert!(command
        .ensure_replayable("update", Some("supplier-1"), FP1)
        .is_ok());
    assert!(command
        .ensure_replayable("update", Some("supplier-1"), FP1_V1)
        .is_ok());
    let replayed = command_view(command.clone());
    assert_eq!(replayed.effective_from, "2026-01-01");
    assert_eq!(replayed.recorded_at, command.base.created_at);
    assert_eq!(replayed.change_reason, "修订");
    assert!(command
        .ensure_replayable("update", Some("supplier-2"), FP1)
        .is_err());
    assert!(command
        .ensure_replayable("update", Some("supplier-1"), FP2)
        .is_err());
    assert!(command
        .ensure_replayable("create", Some("supplier-1"), FP1)
        .is_err());
    assert!(SupplierProfileCommand::ensure_version(3, 3).is_ok());
    assert!(SupplierProfileCommand::ensure_version(3, 2).is_err());
    assert!(matches!(
        SupplierProfileCommand::required_update_version(None, "主体"),
        Err(e) if e.to_string().contains("版本不能为空")
    ));
    assert_eq!(
        SupplierProfileCommand::required_create_identity(Some(" SUP-001 "), "供应商编号").unwrap(),
        "SUP-001"
    );
    let req1 = SaveSupplierProfileRequest {
        idempotency_key: "key-1".to_string(),
        party_no: Some("PARTY-1".to_string()),
        supplier_no: Some("SUP-1".to_string()),
        expected_party_version: None,
        expected_supplier_version: None,
        legal_name: "示例".to_string(),
        short_name: None,
        unified_credit_code: None,
        contact: None,
        clear_contact: false,
        address: None,
        clear_address: false,
        tax_no: None,
        clear_tax_profile: false,
        bank_account: None,
        clear_bank_account: false,
        settlement_mode: entities::supplier::SettlementMode::Prepayment,
        reconciliation_cycle: entities::supplier::ReconciliationCycle::Monthly,
        payment_term_snapshot: "PREPAY_30".to_string(),
        business_category: None,
        invoice_type: entities::supplier::InvoiceType::VatSpecial,
        invoice_tax_rate: entities::money::Rate::from_str("0.13").unwrap(),
        signing_entity_party_id: entities::ids::PartyId::new("party-1"),
        payment_entity_party_id: entities::ids::PartyId::new("party-2"),
        capability_codes: vec![],
        qualifications: vec![],
        rating: None,
        effective_from: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
        change_reason: "首次".to_string(),
    };
    let mut req2 = req1.clone();
    let fp1 = req1.fingerprint().unwrap();
    let fp2 = req2.fingerprint().unwrap();
    assert_eq!(fp1, fp2);
    assert!(fp1.starts_with("sha256-v1:"));
    assert_eq!(fp1.len(), "sha256-v1:".len() + 64);
    req2.legal_name = "不同".to_string();
    let fp3 = req2.fingerprint().unwrap();
    assert_ne!(fp1, fp3);
    assert!(SaveSupplierProfileRequest::required_create_identity(None, "主体编号").is_err());
    assert!(SaveSupplierProfileRequest::ensure_version(1, 2).is_err());
    let digest = fp1.strip_prefix("sha256-v1:").unwrap();
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
    // 兼容旧裸 hex：存储为裸 hex 的命令仍可与新版指纹回放
    let bare = fp1.strip_prefix("sha256-v1:").unwrap().to_string();
    let cmd_bare = SupplierProfileCommand::new(
        "cmd-bare-fp",
        SupplierProfileCommandData {
            idempotency_key: "key-1".to_string(),
            operation: "update".to_string(),
            request_fingerprint: bare.clone(),
            supplier_id: "supplier-1".to_string(),
            supplier_no: "SUP-1".to_string(),
            revision_id: "revision-1".to_string(),
            revision_no: 2,
            supplier_version: 2,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            change_reason: "修订".to_string(),
        },
    )
    .unwrap();
    assert!(cmd_bare
        .ensure_replayable("update", Some("supplier-1"), &fp1)
        .is_ok());
    assert_eq!(cmd_bare.request_fingerprint, bare);
}

use std::str::FromStr;
