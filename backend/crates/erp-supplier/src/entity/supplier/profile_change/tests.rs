use std::collections::HashMap;
use std::str::FromStr;

use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    PartyId, SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
    SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
    SupplierQualificationRevisionId,
};

use super::{
    NewQualificationParams, PlanCommercialProfileRevisionParams, apply_qualification_input, new_capability,
    new_qualification, plan_commercial_profile_revision,
};
use crate::entity::supplier::{
    CapabilityCode, InvoiceType, QualificationStatus, QualificationType, ReconciliationCycle, SettlementMode,
    SupplierAccount, SupplierAccountData, SupplierAccountStatus, SupplierQualification,
    SupplierQualificationData,
};

fn business_date(y: i32, m: u32, d: u32) -> BusinessDate {
    BusinessDate::from_ymd(y, m, d).unwrap()
}

/// 根资料移除日期不完整的启用合同仍须停用；不能用采购资格替代生命周期。
#[test]
fn omitted_unverified_contracts_are_disabled_once() {
    use super::SupplierProfileChangePlan;
    use crate::entity::supplier::SupplierQualificationUpdate;
    for start in [None, Some(business_date(2026, 1, 1))] {
        let mut contract = SupplierQualification::new(
            SupplierQualificationId::new("contract"),
            SupplierQualificationData {
                supplier_id: SupplierAccountId::new("supplier"),
                qualification_type: QualificationType::Contract,
                certificate_no: "HT-1".into(),
                issuer: None,
                valid_from: start,
                valid_to: None,
                attachment_id: None,
                status: QualificationStatus::Active,
            },
            "actor",
        )
        .unwrap();
        assert!(!contract.is_valid());
        let plan = |contract: &SupplierQualification| {
            SupplierProfileChangePlan::from_loaded(
                &[],
                std::slice::from_ref(contract),
                &HashMap::new(),
                &HashMap::new(),
                &[],
                &[],
            )
            .unwrap()
        };
        assert_eq!(plan(&contract).qualification_disables, vec![contract.identity_key()]);
        contract
            .update(
                SupplierQualificationUpdate {
                    status: Some(QualificationStatus::Disabled),
                    ..Default::default()
                },
                "actor",
            )
            .unwrap();
        let revision =
            contract.snapshot_revision(SupplierQualificationRevisionId::new("disabled-revision"), 2).unwrap();
        assert_eq!(revision.status, QualificationStatus::Disabled);
        assert_eq!(revision.valid_from, start);
        assert_eq!(revision.valid_to, None);
        assert!(plan(&contract).qualification_disables.is_empty());
    }
}

/// 覆盖商务资料修订：推进供应商当前指针。
#[test]
fn commercial_profile_revision_advances_pointer() {
    let mut supplier = SupplierAccount::new(
        SupplierAccountId::new("supplier-1"),
        SupplierAccountData {
            party_id: PartyId::new("party-1"),
            supplier_no: "SUP-1".to_string(),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: None,
            maintainer_user_id: "buyer-1".to_string(),
            business_org_unit_id: "org-a".to_string(),
            status: SupplierAccountStatus::Active,
        },
        "admin-1",
    )
    .unwrap();
    let rev = plan_commercial_profile_revision(PlanCommercialProfileRevisionParams {
        supplier: &mut supplier,
        settlement_mode: SettlementMode::Prepayment,
        reconciliation_cycle: ReconciliationCycle::Monthly,
        payment_term_snapshot: "PREPAY_30".to_string(),
        business_category: None,
        invoice_type: InvoiceType::VatSpecial,
        invoice_tax_rate: Some(erp_core::money::Rate::from_str("0.13").unwrap()),
        invoice_tax_rates: None,
        signing_entity_party_id: PartyId::new("party-sign"),
        payment_entity_party_id: PartyId::new("party-pay"),
        change_reason: "首版".to_string(),
        revision_id: SupplierCommercialProfileRevisionId::new("cpr-1"),
        revision_no: 1,
        actor_id: "admin-2",
    })
    .unwrap();
    assert_eq!(rev.revision.revision_no, 1);
    assert_eq!(
        supplier.current_commercial_profile_revision_id.as_ref().map(|id| id.to_string()),
        Some("cpr-1".to_string())
    );
}

/// 覆盖能力启停：新能力首版为 Active 且修订号 1。
#[test]
fn new_capability_creates_active_with_revision_one() {
    let (cap, rev) = new_capability(
        &SupplierAccountId::new("supplier-1"),
        CapabilityCode::Physical,
        business_date(2026, 8, 31),
        "buyer-1",
        "admin-1",
        SupplierCapabilityId::new("cap-1"),
        SupplierCapabilityRevisionId::new("cap-rev-1"),
    )
    .unwrap();
    assert!(cap.is_active());
    assert_eq!(rev.revision.revision_no, 1);
    assert_eq!(cap.stable.current_revision_id.as_deref(), Some("cap-rev-1"));
    assert_eq!(rev.capability_code, CapabilityCode::Physical);
}

/// 覆盖资质字段变化：停用资质经 `apply` 后回 Active 且字段被替换。
#[test]
fn apply_qualification_input_reactivates_and_replaces_fields() {
    let mut q = SupplierQualification::new(
        SupplierQualificationId::new("qual-1"),
        SupplierQualificationData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            qualification_type: QualificationType::Contract,
            certificate_no: "C-001".to_string(),
            issuer: Some("旧机构".to_string()),
            valid_from: Some(business_date(2026, 1, 1)),
            valid_to: None,
            attachment_id: None,
            status: QualificationStatus::Active,
        },
        "admin-1",
    )
    .unwrap();
    // 停用
    q.update(
        crate::entity::supplier::SupplierQualificationUpdate {
            status: Some(QualificationStatus::Disabled),
            ..Default::default()
        },
        "admin-2",
    )
    .unwrap();
    assert!(!q.is_valid());
    apply_qualification_input(
        &mut q,
        Some("新机构".to_string()),
        Some(business_date(2026, 8, 31)),
        Some(business_date(2026, 12, 31)),
        None,
        "admin-3",
    )
    .unwrap();
    assert!(q.is_valid());
    assert_eq!(q.issuer.as_deref(), Some("新机构"));
    assert_eq!(q.valid_from, Some(business_date(2026, 8, 31)));
}

/// 覆盖资质无变化：同样字段再次 apply 应保持幂等（不改变有效状态的重复更新不报错）。
#[test]
fn apply_qualification_input_no_change_keeps_valid() {
    let mut q = SupplierQualification::new(
        SupplierQualificationId::new("qual-2"),
        SupplierQualificationData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            qualification_type: QualificationType::Contract,
            certificate_no: "C-002".to_string(),
            issuer: None,
            valid_from: Some(business_date(2026, 1, 1)),
            valid_to: None,
            attachment_id: None,
            status: QualificationStatus::Active,
        },
        "admin-1",
    )
    .unwrap();
    let before = q.clone();
    apply_qualification_input(&mut q, None, Some(business_date(2026, 1, 1)), None, None, "admin-2").unwrap();
    assert_eq!(q.issuer, before.issuer);
    assert!(!q.is_valid(), "起止日期不完整的合同不能自动变为有效");
    assert!(q.matches_profile_fields(None, q.valid_from, None, None));
}

/// 覆盖重复能力：同一代码重复请求在 Service 层已由 `validate_profile_selection` 拒绝，领域层新建时依赖外部去重；此处验证新资质创建时未启用能力会失败。
#[test]
fn new_qualification_fails_when_capability_missing() {
    let mut cap_ids = HashMap::new();
    cap_ids.insert("physical".to_string(), SupplierCapabilityId::new("cap-1"));
    let err = new_qualification(NewQualificationParams {
        supplier_id: &SupplierAccountId::new("supplier-1"),
        qualification_type: QualificationType::Contract,
        certificate_no: "C-003".to_string(),
        issuer: None,
        valid_from: Some(business_date(2026, 1, 1)),
        valid_to: None,
        attachment_id: None,
        capability_codes: &[CapabilityCode::Api],
        capability_ids: &cap_ids,
        actor_id: "admin-1",
        qualification_id: SupplierQualificationId::new("qual-3"),
        revision_id: SupplierQualificationRevisionId::new("qual-rev-3"),
        link_ids: vec![SupplierQualificationCapabilityId::new("link-1")],
    })
    .unwrap_err();
    assert!(err.to_string().contains("资质适用能力不存在"));
}

/// 覆盖新资质创建：字段与关联一致，首版修订号 1。
#[test]
fn new_qualification_creates_with_links() {
    let mut cap_ids = HashMap::new();
    cap_ids.insert("physical".to_string(), SupplierCapabilityId::new("cap-1"));
    cap_ids.insert("api".to_string(), SupplierCapabilityId::new("cap-2"));
    let (qual, rev, links) = new_qualification(NewQualificationParams {
        supplier_id: &SupplierAccountId::new("supplier-1"),
        qualification_type: QualificationType::Contract,
        certificate_no: "C-004".to_string(),
        issuer: Some("机构".to_string()),
        valid_from: Some(business_date(2026, 1, 1)),
        valid_to: Some(business_date(2026, 12, 31)),
        attachment_id: None,
        capability_codes: &[CapabilityCode::Physical, CapabilityCode::Api],
        capability_ids: &cap_ids,
        actor_id: "admin-1",
        qualification_id: SupplierQualificationId::new("qual-4"),
        revision_id: SupplierQualificationRevisionId::new("qual-rev-4"),
        link_ids: vec![
            SupplierQualificationCapabilityId::new("link-1"),
            SupplierQualificationCapabilityId::new("link-2"),
        ],
    })
    .unwrap();
    assert_eq!(rev.revision.revision_no, 1);
    assert_eq!(links.len(), 2);
    assert_eq!(qual.certificate_no, "C-004");
}

/// 覆盖 link 数量不一致：返回错误。
#[test]
fn new_qualification_rejects_mismatched_link_ids() {
    let cap_ids = HashMap::new();
    let err = new_qualification(NewQualificationParams {
        supplier_id: &SupplierAccountId::new("supplier-1"),
        qualification_type: QualificationType::Contract,
        certificate_no: "C-005".to_string(),
        issuer: None,
        valid_from: Some(business_date(2026, 1, 1)),
        valid_to: None,
        attachment_id: None,
        capability_codes: &[CapabilityCode::Physical],
        capability_ids: &cap_ids,
        actor_id: "admin-1",
        qualification_id: SupplierQualificationId::new("qual-5"),
        revision_id: SupplierQualificationRevisionId::new("qual-rev-5"),
        link_ids: vec![],
    })
    .unwrap_err();
    assert!(err.to_string().contains("关联 ID 数量不一致"));
}

/// 覆盖能力 Disabled→Active 重新启用：先停用再启用保持同一能力身份且可生成新快照。
#[test]
fn capability_disable_then_reenable_via_update() {
    let (mut cap, _) = new_capability(
        &SupplierAccountId::new("supplier-re"),
        CapabilityCode::Api,
        business_date(2026, 1, 1),
        "buyer-1",
        "admin-1",
        SupplierCapabilityId::new("cap-re"),
        SupplierCapabilityRevisionId::new("cap-rev-1"),
    )
    .unwrap();
    assert!(cap.is_active());
    cap.update(
        crate::entity::supplier::SupplierCapabilityUpdate {
            status: Some(crate::entity::supplier::CapabilityStatus::Disabled),
            ..Default::default()
        },
        "admin-2",
    )
    .unwrap();
    assert!(!cap.is_active());
    cap.update(
        crate::entity::supplier::SupplierCapabilityUpdate {
            status: Some(crate::entity::supplier::CapabilityStatus::Active),
            ..Default::default()
        },
        "admin-3",
    )
    .unwrap();
    assert!(cap.is_active());
    let snap = cap.snapshot_revision(SupplierCapabilityRevisionId::new("cap-rev-2"), 2).unwrap();
    assert_eq!(snap.status, crate::entity::supplier::CapabilityStatus::Active);
    assert_eq!(snap.revision.revision_no, 2);
}

/// 覆盖无变化过滤保持 is_valid：相同可变字段与关联集合时不产生更新。
#[test]
fn qualification_no_change_preserves_valid_and_matches_fields() {
    let mut cap_ids = HashMap::new();
    cap_ids.insert("physical".to_string(), SupplierCapabilityId::new("cap-p"));
    let (mut qual, _, _) = new_qualification(NewQualificationParams {
        supplier_id: &SupplierAccountId::new("supplier-nc"),
        qualification_type: QualificationType::Certificate,
        certificate_no: "CERT-NC".to_string(),
        issuer: Some("机构".to_string()),
        valid_from: Some(business_date(2026, 1, 1)),
        valid_to: None,
        attachment_id: None,
        capability_codes: &[CapabilityCode::Physical],
        capability_ids: &cap_ids,
        actor_id: "admin-1",
        qualification_id: SupplierQualificationId::new("qual-nc"),
        revision_id: SupplierQualificationRevisionId::new("qual-rev-nc"),
        link_ids: vec![SupplierQualificationCapabilityId::new("link-nc")],
    })
    .unwrap();
    assert!(qual.is_valid());
    assert!(qual.matches_profile_fields(Some("机构"), Some(business_date(2026, 1, 1)), None, None));
    let before = qual.clone();
    apply_qualification_input(
        &mut qual,
        Some("机构".to_string()),
        Some(business_date(2026, 1, 1)),
        None,
        None,
        "admin-2",
    )
    .unwrap();
    assert!(qual.is_valid());
    assert_eq!(qual.issuer, before.issuer);
    assert_eq!(qual.valid_from, before.valid_from);
}

/// 覆盖重复能力代码由 `validate_profile_selection` 拒绝。
#[test]
fn duplicate_capability_codes_rejected_via_validate_profile_selection() {
    use crate::entity::supplier::{SupplierQualificationSelection, validate_profile_selection};
    let dup = [CapabilityCode::Physical, CapabilityCode::Physical];
    assert!(validate_profile_selection(&dup, &[]).is_err());
    let quals = [SupplierQualificationSelection {
        qualification_type: QualificationType::Contract,
        certificate_no: "DUP-001",
        capability_codes: &[CapabilityCode::Physical],
    }];
    assert!(validate_profile_selection(&dup, &quals).is_err());
    let dup_qual = [
        SupplierQualificationSelection {
            qualification_type: QualificationType::Contract,
            certificate_no: "DUP-002",
            capability_codes: &[],
        },
        SupplierQualificationSelection {
            qualification_type: QualificationType::Contract,
            certificate_no: "DUP-002",
            capability_codes: &[],
        },
    ];
    assert!(validate_profile_selection(&[CapabilityCode::Api], &dup_qual).is_err());
}

/// 覆盖 `SupplierProfileChangePlan::from_loaded` 的完整变更矩阵：
/// 能力启停（含 Disabled→Active 重新启用）、无变化过滤、资质字段与关联比对及新增/停用。
#[test]
fn profile_change_plan_from_loaded_covers_full_matrix() {
    use std::collections::{HashMap, HashSet};

    use erp_core::ids::{
        SupplierAccountId, SupplierCapabilityId, SupplierQualificationCapabilityId, SupplierQualificationId,
        SupplierQualificationRevisionId,
    };

    use super::{PlannedQualificationInput, SupplierProfileChangePlan};
    use crate::entity::supplier::{CapabilityStatus, SupplierCapability, SupplierCapabilityData};

    let supplier_id = SupplierAccountId::new("supplier-plan");
    // Existing capabilities: Physical Active, Api Disabled
    let mut cap_active = SupplierCapability::new(
        SupplierCapabilityId::new("cap-phy"),
        SupplierCapabilityData {
            supplier_id: supplier_id.clone(),
            capability_code: CapabilityCode::Physical,
            service_region: None,
            owner_user_id: "admin".to_string(),
            fulfillment_note: None,
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            status: CapabilityStatus::Active,
        },
        "admin",
    )
    .unwrap();
    cap_active.stable.current_revision_id = Some("rev-phy".to_string());
    let mut cap_disabled = SupplierCapability::new(
        SupplierCapabilityId::new("cap-api"),
        SupplierCapabilityData {
            supplier_id: supplier_id.clone(),
            capability_code: CapabilityCode::Api,
            service_region: None,
            owner_user_id: "admin".to_string(),
            fulfillment_note: None,
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            status: CapabilityStatus::Disabled,
        },
        "admin",
    )
    .unwrap();
    cap_disabled.stable.current_revision_id = Some("rev-api".to_string());
    let capabilities = vec![cap_active.clone(), cap_disabled.clone()];

    // capability_ids: Physical->cap-phy, Api->cap-api (current valid set)
    let mut capability_ids = HashMap::new();
    capability_ids.insert("physical".to_string(), SupplierCapabilityId::new("cap-phy"));
    capability_ids.insert("api".to_string(), SupplierCapabilityId::new("cap-api"));

    // Existing qualifications: one valid matching, one valid to be updated, one valid to be disabled
    let mut cap_ids_for_qual = HashMap::new();
    cap_ids_for_qual.insert("physical".to_string(), SupplierCapabilityId::new("cap-phy"));
    let (qual_match, _, _) = new_qualification(NewQualificationParams {
        supplier_id: &supplier_id,
        qualification_type: QualificationType::Contract,
        certificate_no: "MATCH-001".to_string(),
        issuer: Some("机构".to_string()),
        valid_from: Some(business_date(2026, 1, 1)),
        valid_to: None,
        attachment_id: None,
        capability_codes: &[CapabilityCode::Physical],
        capability_ids: &cap_ids_for_qual,
        actor_id: "admin",
        qualification_id: SupplierQualificationId::new("qual-match"),
        revision_id: SupplierQualificationRevisionId::new("qual-rev-match"),
        link_ids: vec![SupplierQualificationCapabilityId::new("link-match")],
    })
    .unwrap();
    // keep valid
    let (qual_to_update, _, _) = new_qualification(NewQualificationParams {
        supplier_id: &supplier_id,
        qualification_type: QualificationType::Certificate,
        certificate_no: "UPDATE-001".to_string(),
        issuer: Some("旧机构".to_string()),
        valid_from: Some(business_date(2026, 1, 1)),
        valid_to: None,
        attachment_id: None,
        capability_codes: &[CapabilityCode::Physical],
        capability_ids: &cap_ids_for_qual,
        actor_id: "admin",
        qualification_id: SupplierQualificationId::new("qual-update"),
        revision_id: SupplierQualificationRevisionId::new("qual-rev-update"),
        link_ids: vec![SupplierQualificationCapabilityId::new("link-update")],
    })
    .unwrap();
    let (qual_to_disable, _, _) = new_qualification(NewQualificationParams {
        supplier_id: &supplier_id,
        qualification_type: QualificationType::Authorization,
        certificate_no: "DISABLE-001".to_string(),
        issuer: None,
        valid_from: Some(business_date(2026, 1, 1)),
        valid_to: None,
        attachment_id: None,
        capability_codes: &[CapabilityCode::Physical],
        capability_ids: &cap_ids_for_qual,
        actor_id: "admin",
        qualification_id: SupplierQualificationId::new("qual-disable"),
        revision_id: SupplierQualificationRevisionId::new("qual-rev-disable"),
        link_ids: vec![SupplierQualificationCapabilityId::new("link-disable")],
    })
    .unwrap();
    let qualifications = vec![qual_match.clone(), qual_to_update.clone(), qual_to_disable.clone()];
    let mut linked: HashMap<String, HashSet<String>> = HashMap::new();
    linked.insert("qual-match".to_string(), HashSet::from(["cap-phy".to_string()]));
    linked.insert("qual-update".to_string(), HashSet::from(["cap-phy".to_string()]));
    linked.insert("qual-disable".to_string(), HashSet::from(["cap-phy".to_string()]));

    // Requested: capabilities: Physical (still wanted) + Virtual (new) ; Api not requested => should be disabled? Actually Api currently Disabled and not requested => wanted==is_active (false==false) => no toggle. So only disable Physical? Wait Physical is Active and wanted true => no toggle. Api Disabled not wanted => no toggle. Virtual not existing => create.
    // To test Disable, request only Virtual, so Physical Active => toggle to Disabled, Api Disabled stays, Virtual create.
    // But also test re-enable: we want Api Disabled -> wanted true => toggle to Active. So include Api in requested.
    let requested_caps = vec![CapabilityCode::Physical, CapabilityCode::Api, CapabilityCode::Virtual];
    // Now Physical stays, Api re-enable, Virtual create.
    // Add a case where existing Active not requested => toggle to Disabled by using separate plan below
    // Qualifications requested: keep MATCH, update UPDATE with new issuer, create NEW, disable DISABLE not included
    let planned_inputs = vec![
        PlannedQualificationInput {
            qualification_type: QualificationType::Contract,
            certificate_no: "MATCH-001".to_string(),
            issuer: Some("机构".to_string()),
            valid_from: Some(business_date(2026, 1, 1)),
            valid_to: None,
            attachment_id: None,
            capability_codes: vec![CapabilityCode::Physical],
        },
        PlannedQualificationInput {
            qualification_type: QualificationType::Certificate,
            certificate_no: "UPDATE-001".to_string(),
            issuer: Some("新机构".to_string()),
            valid_from: Some(business_date(2026, 1, 1)),
            valid_to: None,
            attachment_id: None,
            capability_codes: vec![CapabilityCode::Physical],
        },
        PlannedQualificationInput {
            qualification_type: QualificationType::Contract,
            certificate_no: "NEW-001".to_string(),
            issuer: None,
            valid_from: Some(business_date(2026, 8, 31)),
            valid_to: None,
            attachment_id: None,
            capability_codes: vec![CapabilityCode::Physical],
        },
    ];
    let plan = SupplierProfileChangePlan::from_loaded(
        &capabilities,
        &qualifications,
        &linked,
        &capability_ids,
        &requested_caps,
        &planned_inputs,
    )
    .unwrap();
    // Capabilities: Api Disabled->Active toggle expected, Physical no toggle, Virtual create
    assert!(
        plan.capability_toggles
            .iter()
            .any(|t| t.code == CapabilityCode::Api && t.target_status == CapabilityStatus::Active)
    );
    assert!(!plan.capability_toggles.iter().any(|t| t.code == CapabilityCode::Physical));
    assert!(plan.capability_creates.contains(&CapabilityCode::Virtual));
    // Qualifications: MATCH no change => not in updates, UPDATE should be in updates, DISABLE should be disabled, NEW in creates
    assert!(!plan.qualification_updates.contains(&qual_match.identity_key()));
    assert!(plan.qualification_updates.contains(&qual_to_update.identity_key()));
    assert!(plan.qualification_disables.contains(&qual_to_disable.identity_key()));
    assert_eq!(plan.qualification_creates.len(), 1);
    assert_eq!(plan.qualification_creates[0].certificate_no, "NEW-001");

    // Additional: capability disable when not wanted
    let plan2 = SupplierProfileChangePlan::from_loaded(
        &capabilities,
        &[],
        &HashMap::new(),
        &HashMap::new(),
        &[CapabilityCode::Virtual],
        &[],
    )
    .unwrap();
    assert!(
        plan2
            .capability_toggles
            .iter()
            .any(|t| t.code == CapabilityCode::Physical && t.target_status == CapabilityStatus::Disabled)
    );
    assert!(!plan2.capability_toggles.iter().any(|t| t.code == CapabilityCode::Api));
    assert!(plan2.capability_creates.contains(&CapabilityCode::Virtual));
}
