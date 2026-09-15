//! 供应商列表/详情共用的最小读模型装配。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use erp_core::ids::PartyId;

use crate::dto::supplier::{CommercialProfileView, SupplierQualificationHealth, SupplierView};
use crate::entity::supplier::{
    CapabilityCode, SupplierCapability, SupplierCommercialProfileRevision, SupplierQualification,
};
use crate::ports::{PartyListFact, PartyRevisionFact};
use crate::repository::SupplierAccountRow;

/// 装配供应商列表视图所需的批量事实。
pub(super) struct SupplierViewAssembleInput {
    /// 当前页或单条供应商投影行。
    pub rows: Vec<SupplierAccountRow>,
    /// 主体列表事实。
    pub parties: Vec<PartyListFact>,
    /// 主体当前名称修订。
    pub revisions: Vec<PartyRevisionFact>,
    /// 当前商务资料版本。
    pub profiles: Vec<SupplierCommercialProfileRevision>,
    /// 当前页能力事实。
    pub capabilities: Vec<SupplierCapability>,
    /// 当前页资质事实。
    pub qualifications: Vec<SupplierQualification>,
    /// 签约/付款主体法定名称，键为主体 ID 字符串。
    pub entity_names: HashMap<String, String>,
    /// 能力与资质判定业务日。
    pub as_of: BusinessDate,
}

/// 将批量读取结果按稳定 ID 装配为供应商列表/详情统一视图。
///
/// # 参数
/// * `input` - 投影行与水合事实
///
/// # 返回
/// 返回与输入行顺序一致的供应商视图。
///
/// # 错误
/// 无。
pub(super) fn assemble_supplier_views(input: SupplierViewAssembleInput) -> Vec<SupplierView> {
    let context = SupplierRowContext {
        parties: party_by_id(input.parties),
        revisions: revision_by_id(input.revisions),
        profiles: profile_by_id(input.profiles),
        capabilities: capability_codes_by_supplier(&input.capabilities, input.as_of),
        qualifications: qualifications_by_supplier(&input.qualifications),
        entity_names: input.entity_names,
        as_of: input.as_of,
    };
    input.rows.into_iter().map(|row| assemble_one_supplier_view(row, &context)).collect()
}

/// 单行装配所需的已建字典。
struct SupplierRowContext<'a> {
    /// 主体字典。
    parties: HashMap<String, PartyListFact>,
    /// 名称修订字典。
    revisions: HashMap<String, PartyRevisionFact>,
    /// 商务资料字典。
    profiles: HashMap<String, SupplierCommercialProfileRevision>,
    /// 供应商到有效能力代码。
    capabilities: HashMap<String, Vec<CapabilityCode>>,
    /// 供应商到资质引用。
    qualifications: HashMap<String, Vec<&'a SupplierQualification>>,
    /// 签约/付款主体名称。
    entity_names: HashMap<String, String>,
    /// 判定业务日。
    as_of: BusinessDate,
}

/// 收集商务版本引用的签约与付款主体 ID。
///
/// # 参数
/// * `profiles` - 当前页商务资料
///
/// # 返回
/// 返回去重后的主体 ID，供批量读取法定名称。
///
/// # 错误
/// 无。
pub(super) fn commercial_party_ids(profiles: &[SupplierCommercialProfileRevision]) -> Vec<PartyId> {
    let mut ids: Vec<String> = profiles
        .iter()
        .flat_map(|profile| {
            [profile.signing_entity_party_id.to_string(), profile.payment_entity_party_id.to_string()]
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids.into_iter().map(|id| PartyId::new(&id)).collect()
}

/// 装配单行供应商视图。
///
/// # 参数
/// * `row` - 供应商投影行
/// * `context` - 已按 ID 建好的水合字典
///
/// # 返回
/// 返回该行的列表视图。
///
/// # 错误
/// 无。
fn assemble_one_supplier_view(row: SupplierAccountRow, context: &SupplierRowContext<'_>) -> SupplierView {
    let party = context.parties.get(&row.party_id);
    let revision =
        party.and_then(|party| party.current_revision_id.as_ref()).and_then(|id| context.revisions.get(id));
    let current_profile = row
        .current_commercial_profile_revision_id
        .as_ref()
        .and_then(|id| context.profiles.get(id))
        .cloned()
        .map(|profile| named_profile(profile, &context.entity_names));
    let quals = context.qualifications.get(&row.id).map(|items| items.as_slice()).unwrap_or(&[]);
    SupplierView {
        id: row.id.clone(),
        party_id: row.party_id,
        party_no: party.map(|party| party.party_no.clone()),
        legal_name: revision.map(|revision| revision.legal_name.clone()),
        short_name: revision.and_then(|revision| revision.short_name.clone()),
        party_version: party.map(|party| party.version),
        supplier_no: row.supplier_no,
        default_payment_term_id: row.default_payment_term_id,
        current_commercial_profile_revision_id: row.current_commercial_profile_revision_id,
        status: row.status,
        version: row.version,
        created_at: row.created_at,
        current_profile,
        capability_codes: context.capabilities.get(&row.id).cloned().unwrap_or_default(),
        qualification_health: Some(SupplierQualificationHealth::from(SupplierQualification::rollup_health(
            quals.iter().copied(),
            context.as_of,
        ))),
        qualification_types: SupplierQualification::registered_types(quals.iter().copied()),
    }
}

/// 把商务资料映射为带主体名称的视图。
///
/// # 参数
/// * `profile` - 商务资料实体
/// * `names` - 主体 ID 到法定名称
///
/// # 返回
/// 返回填好签约/付款主体名称的商务资料视图。
///
/// # 错误
/// 无。
fn named_profile(
    profile: SupplierCommercialProfileRevision,
    names: &HashMap<String, String>,
) -> CommercialProfileView {
    let mut view = CommercialProfileView::from(profile);
    view.signing_entity_name =
        view.signing_entity_party_id.as_ref().and_then(|party_id| names.get(party_id)).cloned();
    view.payment_entity_name =
        view.payment_entity_party_id.as_ref().and_then(|party_id| names.get(party_id)).cloned();
    view
}

/// 按主体 ID 建字典。
///
/// # 参数
/// * `parties` - 主体列表事实
///
/// # 返回
/// 返回主体 ID 到事实的映射。
fn party_by_id(parties: Vec<PartyListFact>) -> HashMap<String, PartyListFact> {
    parties.into_iter().map(|party| (party.id.clone(), party)).collect()
}

/// 按修订 ID 建字典。
///
/// # 参数
/// * `revisions` - 主体名称修订
///
/// # 返回
/// 返回修订 ID 到事实的映射。
fn revision_by_id(revisions: Vec<PartyRevisionFact>) -> HashMap<String, PartyRevisionFact> {
    revisions.into_iter().map(|revision| (revision.id.clone(), revision)).collect()
}

/// 按商务资料 ID 建字典。
///
/// # 参数
/// * `profiles` - 商务资料版本
///
/// # 返回
/// 返回资料 ID 到实体的映射。
fn profile_by_id(
    profiles: Vec<SupplierCommercialProfileRevision>,
) -> HashMap<String, SupplierCommercialProfileRevision> {
    profiles.into_iter().map(|profile| (profile.base.id.clone(), profile)).collect()
}

/// 按供应商折叠当前有效能力代码。
///
/// # 参数
/// * `capabilities` - 当前页能力
/// * `as_of` - 判定业务日
///
/// # 返回
/// 返回供应商 ID 到去重能力代码的映射。
fn capability_codes_by_supplier(
    capabilities: &[SupplierCapability],
    as_of: BusinessDate,
) -> HashMap<String, Vec<CapabilityCode>> {
    let mut grouped: HashMap<String, Vec<&SupplierCapability>> = HashMap::new();
    for capability in capabilities {
        grouped.entry(capability.supplier_id.to_string()).or_default().push(capability);
    }
    grouped
        .into_iter()
        .map(|(supplier_id, items)| (supplier_id, SupplierCapability::list_codes(items, as_of)))
        .collect()
}

/// 按供应商分组资质引用。
///
/// # 参数
/// * `qualifications` - 当前页资质
///
/// # 返回
/// 返回供应商 ID 到资质引用列表的映射。
fn qualifications_by_supplier(
    qualifications: &[SupplierQualification],
) -> HashMap<String, Vec<&SupplierQualification>> {
    let mut grouped: HashMap<String, Vec<&SupplierQualification>> = HashMap::new();
    for qualification in qualifications {
        grouped.entry(qualification.supplier_id.to_string()).or_default().push(qualification);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{
        PartyId, SupplierAccountId, SupplierCapabilityId, SupplierCommercialProfileRevisionId,
        SupplierQualificationId,
    };
    use erp_core::money::Rate;

    use super::{SupplierViewAssembleInput, assemble_supplier_views, commercial_party_ids};
    use crate::entity::supplier::{
        CapabilityCode, CapabilityStatus, InvoiceType, QualificationStatus, QualificationType,
        ReconciliationCycle, SettlementMode, SupplierAccountStatus, SupplierCapability,
        SupplierCapabilityData, SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData,
        SupplierQualification, SupplierQualificationData,
    };
    use crate::ports::{PartyListFact, PartyRevisionFact, PartyStatusFact};
    use crate::repository::SupplierAccountRow;

    /// 返回装配测试业务日。
    fn as_of() -> BusinessDate {
        BusinessDate::from_ymd(2026, 8, 31).unwrap()
    }

    /// 列表装配写入能力、资质健康度与签约主体名称。
    #[test]
    fn assemble_fills_list_projection() {
        let profile = SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("rev-1"),
            SupplierCommercialProfileRevisionData {
                supplier_id: SupplierAccountId::new("sup-1"),
                revision_no: 1,
                settlement_mode: SettlementMode::PayAfterUse,
                reconciliation_cycle: ReconciliationCycle::Monthly,
                payment_term_snapshot: "NET-30".to_string(),
                business_category: Some("礼盒".to_string()),
                invoice_type: InvoiceType::VatSpecial,
                invoice_tax_rate: Some(Rate::from_str("0.13").unwrap()),
                invoice_tax_rates: None,
                signing_entity_party_id: PartyId::new("party-sign"),
                payment_entity_party_id: PartyId::new("party-pay"),
                change_reason: "初始".to_string(),
            },
        )
        .unwrap();
        let capability = SupplierCapability::new(
            SupplierCapabilityId::new("cap-1"),
            SupplierCapabilityData {
                supplier_id: SupplierAccountId::new("sup-1"),
                capability_code: CapabilityCode::Physical,
                service_region: None,
                owner_user_id: "buyer".to_string(),
                fulfillment_note: None,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                status: CapabilityStatus::Active,
            },
            "test",
        )
        .unwrap();
        let qualification = SupplierQualification::new(
            SupplierQualificationId::new("qual-1"),
            SupplierQualificationData {
                supplier_id: SupplierAccountId::new("sup-1"),
                qualification_type: QualificationType::Contract,
                certificate_no: "HT-1".to_string(),
                issuer: None,
                valid_from: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
                valid_to: Some(BusinessDate::from_ymd(2026, 9, 10).unwrap()),
                attachment_id: None,
                status: QualificationStatus::Active,
            },
            "test",
        )
        .unwrap();
        let mut names = HashMap::new();
        names.insert("party-sign".to_string(), "签约公司".to_string());
        names.insert("party-pay".to_string(), "付款公司".to_string());
        let views = assemble_supplier_views(SupplierViewAssembleInput {
            rows: vec![SupplierAccountRow {
                id: "sup-1".to_string(),
                party_id: "party-1".to_string(),
                supplier_no: "SUP-1".to_string(),
                default_payment_term_id: None,
                current_commercial_profile_revision_id: Some("rev-1".to_string()),
                status: SupplierAccountStatus::Active,
                version: 1,
                created_at: 1,
            }],
            parties: vec![PartyListFact {
                id: "party-1".to_string(),
                party_no: "PTY-1".to_string(),
                status: PartyStatusFact::Active,
                unified_credit_code: None,
                version: 1,
                current_revision_id: Some("party-rev-1".to_string()),
            }],
            revisions: vec![PartyRevisionFact {
                id: "party-rev-1".to_string(),
                party_id: "party-1".to_string(),
                legal_name: "华东供应商".to_string(),
                short_name: Some("华东".to_string()),
            }],
            profiles: vec![profile.clone()],
            capabilities: vec![capability],
            qualifications: vec![qualification],
            entity_names: names,
            as_of: as_of(),
        });
        let view = &views[0];
        assert_eq!(view.legal_name.as_deref(), Some("华东供应商"));
        assert_eq!(view.capability_codes, vec![CapabilityCode::Physical]);
        assert_eq!(
            view.qualification_health,
            Some(crate::dto::supplier::SupplierQualificationHealth::Expiring30)
        );
        assert_eq!(view.qualification_types, vec![QualificationType::Contract]);
        let profile_view = view.current_profile.as_ref().expect("商务资料");
        assert_eq!(profile_view.signing_entity_name.as_deref(), Some("签约公司"));
        assert_eq!(profile_view.payment_entity_name.as_deref(), Some("付款公司"));
        let mut party_ids: Vec<String> =
            commercial_party_ids(&[profile]).into_iter().map(|id| id.to_string()).collect();
        party_ids.sort();
        assert_eq!(party_ids, vec!["party-pay".to_string(), "party-sign".to_string()]);
    }
}
