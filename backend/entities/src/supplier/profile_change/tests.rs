use super::{
    apply_qualification_input, disable_addresses, disable_bank_accounts, disable_contacts,
    disable_tax_profiles, new_capability, new_qualification, plan_commercial_profile_revision,
    plan_party_revision,
};
use crate::common::time::BusinessDate;
use crate::ids::{
    PartyId, PartyRevisionId, SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
    SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
    SupplierQualificationRevisionId,
};
use crate::party::{
    status::EffectiveRecordStatus, AddressType, Party, PartyAddress, PartyAddressData, PartyBankAccount,
    PartyBankAccountData, PartyContact, PartyContactData, PartyData, PartyKind, PartyStatus, PartyTaxProfile,
    PartyTaxProfileData,
};
use crate::supplier::{
    CapabilityCode, InvoiceType, QualificationStatus, QualificationType, ReconciliationCycle, SettlementMode,
    SupplierAccount, SupplierAccountData, SupplierAccountStatus, SupplierQualification,
    SupplierQualificationData,
};
use std::collections::HashMap;
use std::str::FromStr;

fn business_date(y: i32, m: u32, d: u32) -> BusinessDate {
    BusinessDate::from_ymd(y, m, d).unwrap()
}

/// 覆盖主体修订：设置法定名称、信用代码并推进当前指针。
#[test]
fn party_revision_updates_code_and_pointer() {
    let mut party = Party::new(
        PartyId::new("party-1"),
        PartyData {
            party_no: "P-001".to_string(),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: None,
            status: PartyStatus::Active,
        },
        "admin-1",
    )
    .unwrap();
    let rev = plan_party_revision(
        &mut party,
        Some("91310000MA1BL4KW9X".to_string()),
        "新法定名".to_string(),
        Some("新简称".to_string()),
        "变更".to_string(),
        PartyRevisionId::new("rev-1"),
        1,
        "admin-2",
    )
    .unwrap();
    assert_eq!(party.unified_credit_code.as_deref(), Some("91310000MA1BL4KW9X"));
    assert_eq!(party.stable.current_revision_id.as_deref(), Some("rev-1"));
    assert_eq!(rev.legal_name, "新法定名");
    assert_eq!(rev.revision.revision_no, 1);
}

/// 覆盖主体修订：`None` 清空信用代码。
#[test]
fn party_revision_clears_code_when_none() {
    let mut party = Party::new(
        PartyId::new("party-2"),
        PartyData {
            party_no: "P-002".to_string(),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: Some("91310000MA1BL4KW9X".to_string()),
            status: PartyStatus::Active,
        },
        "admin-1",
    )
    .unwrap();
    plan_party_revision(
        &mut party,
        None,
        "名".to_string(),
        None,
        "清空".to_string(),
        PartyRevisionId::new("rev-2"),
        2,
        "admin-2",
    )
    .unwrap();
    assert_eq!(party.unified_credit_code, None);
}

/// 覆盖商务资料修订：结算方式与支付条件一致性及指针推进。
#[test]
fn commercial_profile_revision_advances_pointer() {
    let mut supplier = SupplierAccount::new(
        SupplierAccountId::new("supplier-1"),
        SupplierAccountData {
            party_id: PartyId::new("party-1"),
            supplier_no: "SUP-1".to_string(),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: None,
            status: SupplierAccountStatus::Active,
        },
        "admin-1",
    )
    .unwrap();
    let rev = plan_commercial_profile_revision(
        &mut supplier,
        SettlementMode::Prepayment,
        ReconciliationCycle::Monthly,
        "PREPAY_30".to_string(),
        None,
        InvoiceType::VatSpecial,
        crate::money::Rate::from_str("0.13").unwrap(),
        PartyId::new("party-sign"),
        PartyId::new("party-pay"),
        "首版".to_string(),
        SupplierCommercialProfileRevisionId::new("cpr-1"),
        1,
        "admin-2",
    )
    .unwrap();
    assert_eq!(rev.revision.revision_no, 1);
    assert_eq!(
        supplier
            .current_commercial_profile_revision_id
            .as_ref()
            .map(|id| id.to_string()),
        Some("cpr-1".to_string())
    );
}

fn fingerprint_key() -> Vec<u8> {
    b"test-key".to_vec()
}

/// 覆盖旧默认事实停用：仅 `Active` 被置 `Disabled` 且清除 `is_default`。
#[test]
fn disable_contacts_only_active() {
    let mut contacts = vec![
        PartyContact::new(
            crate::ids::PartyContactId::new("c1"),
            PartyContactData {
                party_id: PartyId::new("party-1"),
                contact_name: "张三".to_string(),
                title: None,
                mobile: "13800138000".to_string(),
                telephone: None,
                email: None,
                valid_from: business_date(2026, 1, 1),
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            &fingerprint_key(),
            "admin-1",
        )
        .unwrap(),
        PartyContact::new(
            crate::ids::PartyContactId::new("c2"),
            PartyContactData {
                party_id: PartyId::new("party-1"),
                contact_name: "李四".to_string(),
                title: None,
                mobile: "13800138001".to_string(),
                telephone: None,
                email: None,
                valid_from: business_date(2026, 1, 1),
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            &fingerprint_key(),
            "admin-1",
        )
        .unwrap(),
    ];
    // 手动构造一个已停用记录应被 retain 过滤
    let mut disabled = PartyContact::new(
        crate::ids::PartyContactId::new("c3"),
        PartyContactData {
            party_id: PartyId::new("party-1"),
            contact_name: "王五".to_string(),
            title: None,
            mobile: "13800138002".to_string(),
            telephone: None,
            email: None,
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            is_default: false,
            status: EffectiveRecordStatus::Active,
        },
        &fingerprint_key(),
        "admin-1",
    )
    .unwrap();
    disabled
        .update(
            crate::party::PartyContactUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: crate::field_update::FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            "admin-1",
        )
        .unwrap();
    contacts.push(disabled);
    assert_eq!(contacts.len(), 3);
    disable_contacts(&mut contacts, "admin-2").unwrap();
    // 已停用被 retain 过滤，仅剩 2 条且均为 Disabled
    assert_eq!(contacts.len(), 2);
    assert!(contacts
        .iter()
        .all(|c| c.status == EffectiveRecordStatus::Disabled));
    assert!(contacts.iter().all(|c| !c.is_default));
}

/// 覆盖地址/税务/银行停用：与联系人同构。
#[test]
fn disable_addresses_tax_bank() {
    let mut addresses = vec![PartyAddress::new(
        crate::ids::PartyAddressId::new("a1"),
        PartyAddressData {
            party_id: PartyId::new("party-1"),
            address_type: AddressType::Operating,
            contact_name: None,
            address: "地址明文".to_string(),
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        &fingerprint_key(),
        "admin-1",
    )
    .unwrap()];
    disable_addresses(&mut addresses, "admin-2").unwrap();
    assert_eq!(addresses[0].status, EffectiveRecordStatus::Disabled);

    let mut tax = vec![PartyTaxProfile::new(
        crate::ids::PartyTaxProfileId::new("t1"),
        PartyTaxProfileData {
            party_id: PartyId::new("party-1"),
            tax_no: "TAX001".to_string(),
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        "admin-1",
    )
    .unwrap()];
    disable_tax_profiles(&mut tax, "admin-2").unwrap();
    assert_eq!(tax[0].status, EffectiveRecordStatus::Disabled);

    let mut banks = vec![PartyBankAccount::new(
        crate::ids::PartyBankAccountId::new("b1"),
        PartyBankAccountData {
            bank_account_no: "BA-1".to_string(),
            party_id: PartyId::new("party-1"),
            account_name: "示例".to_string(),
            bank_name: "工行".to_string(),
            bank_branch_name: None,
            account_number: "622000000000".to_string(),
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        &fingerprint_key(),
        "admin-1",
    )
    .unwrap()];
    disable_bank_accounts(&mut banks, "admin-2").unwrap();
    assert_eq!(banks[0].status, EffectiveRecordStatus::Disabled);
}

/// 覆盖能力启停：新能力首版为 Active 且修订号 1。
#[test]
fn new_capability_creates_active_with_revision_one() {
    let (cap, rev) = new_capability(
        &SupplierAccountId::new("supplier-1"),
        CapabilityCode::Physical,
        business_date(2026, 8, 31),
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
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            attachment_id: None,
            status: QualificationStatus::Active,
        },
        "admin-1",
    )
    .unwrap();
    // 停用
    q.update(
        crate::supplier::SupplierQualificationUpdate {
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
        business_date(2026, 8, 31),
        Some(business_date(2026, 12, 31)),
        None,
        "admin-3",
    )
    .unwrap();
    assert!(q.is_valid());
    assert_eq!(q.issuer.as_deref(), Some("新机构"));
    assert_eq!(q.valid_from, business_date(2026, 8, 31));
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
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            attachment_id: None,
            status: QualificationStatus::Active,
        },
        "admin-1",
    )
    .unwrap();
    let before = q.clone();
    apply_qualification_input(&mut q, None, business_date(2026, 1, 1), None, None, "admin-2").unwrap();
    assert_eq!(q.issuer, before.issuer);
    assert!(q.is_valid());
}

/// 覆盖重复能力：同一代码重复请求在 Service 层已由 `validate_profile_selection` 拒绝，领域层新建时依赖外部去重；此处验证新资质创建时未启用能力会失败。
#[test]
fn new_qualification_fails_when_capability_missing() {
    let mut cap_ids = HashMap::new();
    cap_ids.insert("physical".to_string(), SupplierCapabilityId::new("cap-1"));
    let err = new_qualification(
        &SupplierAccountId::new("supplier-1"),
        QualificationType::Contract,
        "C-003".to_string(),
        None,
        business_date(2026, 1, 1),
        None,
        None,
        &[CapabilityCode::Api],
        &cap_ids,
        "admin-1",
        SupplierQualificationId::new("qual-3"),
        SupplierQualificationRevisionId::new("qual-rev-3"),
        vec![SupplierQualificationCapabilityId::new("link-1")],
    )
    .unwrap_err();
    assert!(err.to_string().contains("资质适用能力不存在"));
}

/// 覆盖新资质创建：字段与关联一致，首版修订号 1。
#[test]
fn new_qualification_creates_with_links() {
    let mut cap_ids = HashMap::new();
    cap_ids.insert("physical".to_string(), SupplierCapabilityId::new("cap-1"));
    cap_ids.insert("api".to_string(), SupplierCapabilityId::new("cap-2"));
    let (qual, rev, links) = new_qualification(
        &SupplierAccountId::new("supplier-1"),
        QualificationType::Contract,
        "C-004".to_string(),
        Some("机构".to_string()),
        business_date(2026, 1, 1),
        Some(business_date(2026, 12, 31)),
        None,
        &[CapabilityCode::Physical, CapabilityCode::Api],
        &cap_ids,
        "admin-1",
        SupplierQualificationId::new("qual-4"),
        SupplierQualificationRevisionId::new("qual-rev-4"),
        vec![
            SupplierQualificationCapabilityId::new("link-1"),
            SupplierQualificationCapabilityId::new("link-2"),
        ],
    )
    .unwrap();
    assert_eq!(rev.revision.revision_no, 1);
    assert_eq!(links.len(), 2);
    assert_eq!(qual.certificate_no, "C-004");
}

/// 覆盖 link 数量不一致：返回错误。
#[test]
fn new_qualification_rejects_mismatched_link_ids() {
    let cap_ids = HashMap::new();
    let err = new_qualification(
        &SupplierAccountId::new("supplier-1"),
        QualificationType::Contract,
        "C-005".to_string(),
        None,
        business_date(2026, 1, 1),
        None,
        None,
        &[CapabilityCode::Physical],
        &cap_ids,
        "admin-1",
        SupplierQualificationId::new("qual-5"),
        SupplierQualificationRevisionId::new("qual-rev-5"),
        vec![],
    )
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
        "admin-1",
        SupplierCapabilityId::new("cap-re"),
        SupplierCapabilityRevisionId::new("cap-rev-1"),
    )
    .unwrap();
    assert!(cap.is_active());
    cap.update(
        crate::supplier::SupplierCapabilityUpdate {
            status: Some(crate::supplier::CapabilityStatus::Disabled),
            ..Default::default()
        },
        "admin-2",
    )
    .unwrap();
    assert!(!cap.is_active());
    cap.update(
        crate::supplier::SupplierCapabilityUpdate {
            status: Some(crate::supplier::CapabilityStatus::Active),
            ..Default::default()
        },
        "admin-3",
    )
    .unwrap();
    assert!(cap.is_active());
    let snap = cap
        .snapshot_revision(SupplierCapabilityRevisionId::new("cap-rev-2"), 2)
        .unwrap();
    assert_eq!(snap.status, crate::supplier::CapabilityStatus::Active);
    assert_eq!(snap.revision.revision_no, 2);
}

/// 覆盖联系人/地址/税务/银行的 Clear 意图：disable 后集合仅保留 Disabled 已停用状态且可重建新 Active。
#[test]
fn disable_then_new_contact_reenable() {
    let mut contacts = vec![PartyContact::new(
        crate::ids::PartyContactId::new("c-re"),
        PartyContactData {
            party_id: PartyId::new("party-re"),
            contact_name: "张三".to_string(),
            title: None,
            mobile: "13800138000".to_string(),
            telephone: None,
            email: None,
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        &fingerprint_key(),
        "admin-1",
    )
    .unwrap()];
    disable_contacts(&mut contacts, "admin-2").unwrap();
    assert_eq!(contacts[0].status, EffectiveRecordStatus::Disabled);
    assert!(!contacts[0].is_default);
    let recreated = PartyContact::new(
        crate::ids::PartyContactId::new("c-re2"),
        PartyContactData {
            party_id: PartyId::new("party-re"),
            contact_name: "李四".to_string(),
            title: None,
            mobile: "13800138001".to_string(),
            telephone: None,
            email: None,
            valid_from: business_date(2026, 8, 31),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        &fingerprint_key(),
        "admin-3",
    )
    .unwrap();
    assert!(recreated.is_active());
    assert!(recreated.is_default);
}

/// 覆盖地址、税务、银行的 Clear 禁用后保持一致语义。
#[test]
fn clear_intent_for_address_tax_bank_preserves_disable_semantics() {
    let mut addresses = vec![PartyAddress::new(
        crate::ids::PartyAddressId::new("addr-clear"),
        PartyAddressData {
            party_id: PartyId::new("party-clear"),
            address_type: AddressType::Operating,
            contact_name: None,
            address: "原地址".to_string(),
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        &fingerprint_key(),
        "admin-1",
    )
    .unwrap()];
    disable_addresses(&mut addresses, "admin-2").unwrap();
    assert_eq!(addresses[0].status, EffectiveRecordStatus::Disabled);
    let mut taxes = vec![PartyTaxProfile::new(
        crate::ids::PartyTaxProfileId::new("tax-clear"),
        PartyTaxProfileData {
            party_id: PartyId::new("party-clear"),
            tax_no: "91310000MA1BL4KW9X".to_string(),
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        "admin-1",
    )
    .unwrap()];
    disable_tax_profiles(&mut taxes, "admin-2").unwrap();
    assert_eq!(taxes[0].status, EffectiveRecordStatus::Disabled);
    let mut banks = vec![PartyBankAccount::new(
        crate::ids::PartyBankAccountId::new("bank-clear"),
        PartyBankAccountData {
            bank_account_no: "BA-CLEAR".to_string(),
            party_id: PartyId::new("party-clear"),
            account_name: "示例".to_string(),
            bank_name: "工行".to_string(),
            bank_branch_name: None,
            account_number: "622000".to_string(),
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        &fingerprint_key(),
        "admin-1",
    )
    .unwrap()];
    disable_bank_accounts(&mut banks, "admin-2").unwrap();
    assert_eq!(banks[0].status, EffectiveRecordStatus::Disabled);
}

/// 覆盖无变化过滤保持 is_valid：相同可变字段与关联集合时不产生更新。
#[test]
fn qualification_no_change_preserves_valid_and_matches_fields() {
    let mut cap_ids = HashMap::new();
    cap_ids.insert("physical".to_string(), SupplierCapabilityId::new("cap-p"));
    let (mut qual, _, _) = new_qualification(
        &SupplierAccountId::new("supplier-nc"),
        QualificationType::Certificate,
        "CERT-NC".to_string(),
        Some("机构".to_string()),
        business_date(2026, 1, 1),
        None,
        None,
        &[CapabilityCode::Physical],
        &cap_ids,
        "admin-1",
        SupplierQualificationId::new("qual-nc"),
        SupplierQualificationRevisionId::new("qual-rev-nc"),
        vec![SupplierQualificationCapabilityId::new("link-nc")],
    )
    .unwrap();
    assert!(qual.is_valid());
    assert!(qual.matches_profile_fields(Some("机构"), business_date(2026, 1, 1), None, None));
    let before = qual.clone();
    apply_qualification_input(
        &mut qual,
        Some("机构".to_string()),
        business_date(2026, 1, 1),
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
    use crate::supplier::{validate_profile_selection, SupplierQualificationSelection};
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
    use super::{PlannedQualificationInput, SupplierProfileChangePlan};
    use crate::ids::{
        SupplierAccountId, SupplierCapabilityId, SupplierQualificationCapabilityId, SupplierQualificationId,
        SupplierQualificationRevisionId,
    };
    use crate::supplier::{CapabilityStatus, SupplierCapability, SupplierCapabilityData};
    use std::collections::{HashMap, HashSet};

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
    let (qual_match, _, _) = new_qualification(
        &supplier_id,
        QualificationType::Contract,
        "MATCH-001".to_string(),
        Some("机构".to_string()),
        business_date(2026, 1, 1),
        None,
        None,
        &[CapabilityCode::Physical],
        &cap_ids_for_qual,
        "admin",
        SupplierQualificationId::new("qual-match"),
        SupplierQualificationRevisionId::new("qual-rev-match"),
        vec![SupplierQualificationCapabilityId::new("link-match")],
    )
    .unwrap();
    // keep valid
    let (qual_to_update, _, _) = new_qualification(
        &supplier_id,
        QualificationType::Certificate,
        "UPDATE-001".to_string(),
        Some("旧机构".to_string()),
        business_date(2026, 1, 1),
        None,
        None,
        &[CapabilityCode::Physical],
        &cap_ids_for_qual,
        "admin",
        SupplierQualificationId::new("qual-update"),
        SupplierQualificationRevisionId::new("qual-rev-update"),
        vec![SupplierQualificationCapabilityId::new("link-update")],
    )
    .unwrap();
    let (qual_to_disable, _, _) = new_qualification(
        &supplier_id,
        QualificationType::Authorization,
        "DISABLE-001".to_string(),
        None,
        business_date(2026, 1, 1),
        None,
        None,
        &[CapabilityCode::Physical],
        &cap_ids_for_qual,
        "admin",
        SupplierQualificationId::new("qual-disable"),
        SupplierQualificationRevisionId::new("qual-rev-disable"),
        vec![SupplierQualificationCapabilityId::new("link-disable")],
    )
    .unwrap();
    let qualifications = vec![
        qual_match.clone(),
        qual_to_update.clone(),
        qual_to_disable.clone(),
    ];
    let mut linked: HashMap<String, HashSet<String>> = HashMap::new();
    linked.insert("qual-match".to_string(), HashSet::from(["cap-phy".to_string()]));
    linked.insert("qual-update".to_string(), HashSet::from(["cap-phy".to_string()]));
    linked.insert("qual-disable".to_string(), HashSet::from(["cap-phy".to_string()]));

    // Requested: capabilities: Physical (still wanted) + Virtual (new) ; Api not requested => should be disabled? Actually Api currently Disabled and not requested => wanted==is_active (false==false) => no toggle. So only disable Physical? Wait Physical is Active and wanted true => no toggle. Api Disabled not wanted => no toggle. Virtual not existing => create.
    // To test Disable, request only Virtual, so Physical Active => toggle to Disabled, Api Disabled stays, Virtual create.
    // But also test re-enable: we want Api Disabled -> wanted true => toggle to Active. So include Api in requested.
    let requested_caps = vec![
        CapabilityCode::Physical,
        CapabilityCode::Api,
        CapabilityCode::Virtual,
    ];
    // Now Physical stays, Api re-enable, Virtual create.
    // Add a case where existing Active not requested => toggle to Disabled by using separate plan below
    // Qualifications requested: keep MATCH, update UPDATE with new issuer, create NEW, disable DISABLE not included
    let planned_inputs = vec![
        PlannedQualificationInput {
            qualification_type: QualificationType::Contract,
            certificate_no: "MATCH-001".to_string(),
            issuer: Some("机构".to_string()),
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            attachment_id: None,
            capability_codes: vec![CapabilityCode::Physical],
        },
        PlannedQualificationInput {
            qualification_type: QualificationType::Certificate,
            certificate_no: "UPDATE-001".to_string(),
            issuer: Some("新机构".to_string()),
            valid_from: business_date(2026, 1, 1),
            valid_to: None,
            attachment_id: None,
            capability_codes: vec![CapabilityCode::Physical],
        },
        PlannedQualificationInput {
            qualification_type: QualificationType::Contract,
            certificate_no: "NEW-001".to_string(),
            issuer: None,
            valid_from: business_date(2026, 8, 31),
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
    assert!(plan
        .capability_toggles
        .iter()
        .any(|t| t.code == CapabilityCode::Api && t.target_status == CapabilityStatus::Active));
    assert!(!plan
        .capability_toggles
        .iter()
        .any(|t| t.code == CapabilityCode::Physical));
    assert!(plan.capability_creates.contains(&CapabilityCode::Virtual));
    // Qualifications: MATCH no change => not in updates, UPDATE should be in updates, DISABLE should be disabled, NEW in creates
    assert!(!plan.qualification_updates.contains(&qual_match.identity_key()));
    assert!(plan
        .qualification_updates
        .contains(&qual_to_update.identity_key()));
    assert!(plan
        .qualification_disables
        .contains(&qual_to_disable.identity_key()));
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
    assert!(plan2
        .capability_toggles
        .iter()
        .any(|t| t.code == CapabilityCode::Physical && t.target_status == CapabilityStatus::Disabled));
    assert!(!plan2
        .capability_toggles
        .iter()
        .any(|t| t.code == CapabilityCode::Api));
    assert!(plan2.capability_creates.contains(&CapabilityCode::Virtual));
}
