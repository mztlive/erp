//! Supplier-domain helpers for profile command views.
//!
//! Root create/update orchestration that opens a transaction across Party,
//! attachments and audit lives in `erp-processes::supplier_profile`.

use crate::dto::supplier::SupplierProfileMutationView;
use crate::entity::supplier::SupplierProfileCommand;

/// 将命令实体转换为稳定 HTTP 结果。
///
/// # 参数
/// * `command` - 已成功的供应商资料根命令
///
/// # 返回
/// 返回与原 HTTP 契约一致的稳定业务视图。
pub fn command_view(command: SupplierProfileCommand) -> SupplierProfileMutationView {
    SupplierProfileMutationView {
        supplier_id: command.supplier_id,
        supplier_no: command.supplier_no,
        revision_id: command.revision_id,
        revision_no: command.revision_no,
        supplier_version: command.supplier_version,
        effective_from: command.effective_from.to_string(),
        recorded_at: command.base.created_at,
        change_reason: command.change_reason,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;

    use super::command_view;
    use crate::dto::supplier::SaveSupplierProfileRequest;
    use crate::entity::supplier::{
        InvoiceType, ReconciliationCycle, SettlementMode, SupplierProfileCommand, SupplierProfileCommandData,
    };

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
        assert!(command.ensure_replayable("update", Some("supplier-1"), FP1).is_ok());
        assert!(command.ensure_replayable("update", Some("supplier-1"), FP1_V1).is_ok());
        let replayed = command_view(command.clone());
        assert_eq!(replayed.effective_from, "2026-01-01");
        assert_eq!(replayed.recorded_at, command.base.created_at);
        assert_eq!(replayed.change_reason, "修订");
        assert!(command.ensure_replayable("update", Some("supplier-2"), FP1).is_err());
        assert!(command.ensure_replayable("update", Some("supplier-1"), FP2).is_err());
        assert!(command.ensure_replayable("create", Some("supplier-1"), FP1).is_err());
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
            settlement_mode: SettlementMode::Prepayment,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: "PREPAY_30".to_string(),
            business_category: None,
            invoice_type: InvoiceType::VatSpecial,
            invoice_tax_rate: Some(erp_core::money::Rate::from_str("0.13").unwrap()),
            invoice_tax_rates: None,
            signing_entity_party_id: erp_core::ids::PartyId::new("party-1"),
            payment_entity_party_id: erp_core::ids::PartyId::new("party-2"),
            capability_codes: Vec::new(),
            qualifications: Vec::new(),
            rating: None,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            change_reason: "首次".to_string(),
        };
        let req2 = req1.clone();
        assert_eq!(req1.fingerprint().unwrap(), req2.fingerprint().unwrap());
        let mut other = req1.clone();
        other.legal_name = "其他".to_string();
        assert_ne!(req1.fingerprint().unwrap(), other.fingerprint().unwrap());
    }
}
