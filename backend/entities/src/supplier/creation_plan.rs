//! 供应商资料创建领域工厂（`PROC-E05`）。
//!
//! 集中根资料创建时的 Party、Supplier、能力、资质与评级稳定构造；
//! 不触及 MongoDB、HTTP、全局时钟、全局 ID 生成器或原始密钥，
//! Service 显式注入已分配 ID、审计操作人与业务日。

use std::collections::HashMap;

use crate::common::time::BusinessDate;
use crate::ids::{
    PartyId, PartyRevisionId, SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
    SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
    SupplierQualificationRevisionId, SupplierRatingRevisionId,
};
use crate::money::Rate;
use crate::party::{Party, PartyData, PartyKind, PartyRevision, PartyRevisionData, PartyStatus};
use crate::supplier::{
    profile_change, validate_profile_selection, CapabilityCode, InvoiceType, QualificationType,
    ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountData, SupplierAccountStatus,
    SupplierCapability, SupplierCapabilityRevision, SupplierCommercialProfileRevision,
    SupplierCommercialProfileRevisionData, SupplierQualification, SupplierQualificationCapability,
    SupplierQualificationRevision, SupplierQualificationSelection, SupplierRating, SupplierRatingRevision,
    SupplierRatingRevisionData,
};

/// 单份资质创建所需的已分配主键。
///
/// # 约束
/// 纯数据容器，不触及 I/O；`link_ids` 顺序与对应资质输入的
/// `capability_codes` 一一对应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierCreationQualificationIds {
    /// 新资质稳定主键，由 Service 分配。
    pub qualification_id: SupplierQualificationId,
    /// 首版修订主键，由 Service 分配。
    pub revision_id: SupplierQualificationRevisionId,
    /// 适用能力关联主键列表，由 Service 分配。
    pub link_ids: Vec<SupplierQualificationCapabilityId>,
}

/// 根资料创建所需的全部已分配主键。
///
/// # 约束
/// 纯数据容器；所有 ID 均由 Service 通过 ID 生成器分配后传入，
/// 领域层不生成 ID、不读取时钟。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierCreationIds {
    /// 新主体主键。
    pub party_id: PartyId,
    /// 主体首版修订主键。
    pub party_revision_id: PartyRevisionId,
    /// 新供应商角色主键。
    pub supplier_id: SupplierAccountId,
    /// 首版商务资料主键。
    pub commercial_profile_id: SupplierCommercialProfileRevisionId,
    /// 能力代码到已分配能力与修订主键的映射输入。
    pub capability_ids: Vec<(CapabilityCode, SupplierCapabilityId, SupplierCapabilityRevisionId)>,
    /// 每份资质的已分配主键，与输入资质顺序一一对应。
    pub qualification_ids: Vec<SupplierCreationQualificationIds>,
    /// 首版评级主键；`None` 表示不写评级。
    pub rating_id: Option<SupplierRatingRevisionId>,
}

/// 单份资质创建的已准备输入值。
///
/// # 约束
/// 仅携带领域构造所需的已校验值；文件资产存在性与敏感级别由 Service 前置校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierCreationQualificationInput {
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书编号。
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效日。
    pub valid_from: BusinessDate,
    /// 失效日。
    pub valid_to: Option<BusinessDate>,
    /// 附件 ID。
    pub attachment_id: Option<crate::ids::FileAssetId>,
    /// 适用能力代码。
    pub capability_codes: Vec<CapabilityCode>,
}

/// 首版评级创建的已准备输入值。
///
/// # 约束
/// 仅携带领域构造所需的已校验值；`revision_no` 恒为 1，`valid_to` 恒为 `None`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupplierCreationRatingInput {
    /// 期初评分。
    pub initial_score: Option<u8>,
    /// 评级。
    pub rating: SupplierRating,
    /// 当前评分。
    pub current_score: u8,
    /// 生效日。
    pub valid_from: BusinessDate,
}

/// 根资料创建的已准备输入值。
///
/// # 约束
/// 全部字段均为 Service 已校验的业务值；ID、审计与加密结果由 Service 另行
/// 通过 [`SupplierCreationIds`] 与 `actor_id` 注入，不在本结构内生成。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierCreationInputs {
    /// 主体编号。
    pub party_no: String,
    /// 供应商编号。
    pub supplier_no: String,
    /// 法定名称。
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 结算方式。
    pub settlement_mode: SettlementMode,
    /// 对账周期。
    pub reconciliation_cycle: ReconciliationCycle,
    /// 付款条件快照。
    pub payment_term_snapshot: String,
    /// 经营类目。
    pub business_category: Option<String>,
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点。
    pub invoice_tax_rate: Rate,
    /// 签约主体。
    pub signing_entity_party_id: PartyId,
    /// 付款主体。
    pub payment_entity_party_id: PartyId,
    /// 启用的能力代码。
    pub capability_codes: Vec<CapabilityCode>,
    /// 资质输入。
    pub qualifications: Vec<SupplierCreationQualificationInput>,
    /// 评级输入；`None` 表示不写评级。
    pub rating: Option<SupplierCreationRatingInput>,
    /// 从属事实生效日。
    pub effective_from: BusinessDate,
    /// 变更原因。
    pub change_reason: String,
    /// 操作人 ID。
    pub actor_id: String,
}

/// 根资料创建的合法聚合计划。
///
/// # 约束
/// 一次性返回全部待写实体；任一构造失败时不返回部分计划，
/// 调用方不得落库任何子集。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierCreationPlan {
    /// 新主体。
    pub party: Party,
    /// 主体首版修订。
    pub party_revision: PartyRevision,
    /// 新供应商角色。
    pub supplier: SupplierAccount,
    /// 首版商务资料。
    pub commercial_profile: SupplierCommercialProfileRevision,
    /// 新能力集合。
    pub capabilities: Vec<SupplierCapability>,
    /// 能力首版修订集合。
    pub capability_revisions: Vec<SupplierCapabilityRevision>,
    /// 能力代码到稳定 ID 的映射。
    pub capability_ids: HashMap<String, SupplierCapabilityId>,
    /// 新资质集合。
    pub qualifications: Vec<SupplierQualification>,
    /// 资质首版修订集合。
    pub qualification_revisions: Vec<SupplierQualificationRevision>,
    /// 资质适用能力关联集合。
    pub qualification_links: Vec<SupplierQualificationCapability>,
    /// 首版评级；`None` 表示不写评级。
    pub rating: Option<SupplierRatingRevision>,
}

/// 按已准备值与已分配 ID 生成合法的根资料创建聚合。
///
/// 首版修订号恒为 1；主体与供应商当前指针在构造内推进；
/// 能力代码映射与资质适用关联由领域工厂逐项校验。
///
/// # 参数
/// * `ids` - Service 已分配的全部主键，能力与资质 ID 与输入顺序对应
/// * `inputs` - Service 已校验的根资料业务值与操作人
///
/// # 返回
/// 返回一次性待写的合法聚合计划；调用方应在同一事务内整体落库。
///
/// # 错误
/// 能力重复、资质身份重复、资质引用未勾选能力、任一实体字段校验失败、
/// 能力与资质 ID 数量不匹配或评级校验失败时返回错误，且不返回部分计划。
///
/// # 约束
/// 纯内存构造，不触及 MongoDB、HTTP、全局时钟、全局 ID 生成器或原始密钥；
/// 加密密文与文件资产登记由 Service 另行处理。
pub fn plan_supplier_creation(
    ids: SupplierCreationIds,
    inputs: SupplierCreationInputs,
) -> crate::Result<SupplierCreationPlan> {
    let selections: Vec<SupplierQualificationSelection<'_>> = inputs
        .qualifications
        .iter()
        .map(|item| SupplierQualificationSelection {
            qualification_type: item.qualification_type,
            certificate_no: &item.certificate_no,
            capability_codes: &item.capability_codes,
        })
        .collect();
    validate_profile_selection(&inputs.capability_codes, &selections)?;
    if ids.capability_ids.len() != inputs.capability_codes.len() {
        return Err(crate::Error::from("供应商能力 ID 与能力代码数量不一致"));
    }
    if ids.qualification_ids.len() != inputs.qualifications.len() {
        return Err(crate::Error::from("供应商资质 ID 与资质输入数量不一致"));
    }

    let mut party = Party::new(
        ids.party_id.clone(),
        PartyData {
            party_no: inputs.party_no,
            party_kind: PartyKind::Enterprise,
            unified_credit_code: inputs.unified_credit_code,
            status: PartyStatus::Active,
        },
        inputs.actor_id.clone(),
    )?;
    party.stable.current_revision_id = Some(ids.party_revision_id.to_string());
    let party_revision = PartyRevision::new(
        ids.party_revision_id,
        PartyRevisionData {
            party_id: ids.party_id,
            revision_no: 1,
            legal_name: inputs.legal_name,
            short_name: inputs.short_name,
            change_reason: inputs.change_reason.clone(),
        },
    )?;

    let supplier = SupplierAccount::new(
        ids.supplier_id.clone(),
        SupplierAccountData {
            party_id: party.base.id.clone().into(),
            supplier_no: inputs.supplier_no,
            default_payment_term_id: None,
            current_commercial_profile_revision_id: Some(ids.commercial_profile_id.clone()),
            status: SupplierAccountStatus::Active,
        },
        inputs.actor_id.clone(),
    )?;
    let commercial_profile = SupplierCommercialProfileRevision::new(
        ids.commercial_profile_id,
        SupplierCommercialProfileRevisionData {
            supplier_id: ids.supplier_id.clone(),
            revision_no: 1,
            settlement_mode: inputs.settlement_mode,
            reconciliation_cycle: inputs.reconciliation_cycle,
            payment_term_snapshot: inputs.payment_term_snapshot,
            business_category: inputs.business_category,
            invoice_type: inputs.invoice_type,
            invoice_tax_rate: inputs.invoice_tax_rate,
            signing_entity_party_id: inputs.signing_entity_party_id,
            payment_entity_party_id: inputs.payment_entity_party_id,
            change_reason: inputs.change_reason.clone(),
        },
    )?;

    let mut capabilities = Vec::with_capacity(inputs.capability_codes.len());
    let mut capability_revisions = Vec::with_capacity(inputs.capability_codes.len());
    let mut capability_ids = HashMap::new();
    for (code, allocated) in inputs.capability_codes.iter().copied().zip(ids.capability_ids) {
        let (_, capability_id, revision_id) = allocated;
        let (capability, revision) = profile_change::new_capability(
            &ids.supplier_id,
            code,
            inputs.effective_from,
            &inputs.actor_id,
            capability_id.clone(),
            revision_id,
        )?;
        capability_ids.insert(code.as_str().to_string(), capability_id);
        capabilities.push(capability);
        capability_revisions.push(revision);
    }

    let mut qualifications = Vec::with_capacity(inputs.qualifications.len());
    let mut qualification_revisions = Vec::with_capacity(inputs.qualifications.len());
    let mut qualification_links = Vec::new();
    for (input, allocated) in inputs.qualifications.into_iter().zip(ids.qualification_ids) {
        let (qualification, revision, links) = profile_change::new_qualification(
            &ids.supplier_id,
            input.qualification_type,
            input.certificate_no,
            input.issuer,
            input.valid_from,
            input.valid_to,
            input.attachment_id,
            &input.capability_codes,
            &capability_ids,
            &inputs.actor_id,
            allocated.qualification_id,
            allocated.revision_id,
            allocated.link_ids,
        )?;
        qualifications.push(qualification);
        qualification_revisions.push(revision);
        qualification_links.extend(links);
    }

    let rating = match (inputs.rating, ids.rating_id) {
        (Some(input), Some(rating_id)) => Some(SupplierRatingRevision::new(
            rating_id,
            SupplierRatingRevisionData {
                supplier_id: ids.supplier_id.clone(),
                revision_no: 1,
                initial_score: input.initial_score,
                rating: input.rating,
                current_score: input.current_score,
                valid_from: input.valid_from,
                valid_to: None,
                change_reason: inputs.change_reason,
            },
        )?),
        (None, None) => None,
        (Some(_), None) => {
            return Err(crate::Error::from("供应商评级 ID 缺失"));
        }
        (None, Some(_)) => {
            return Err(crate::Error::from("供应商评级输入缺失"));
        }
    };

    Ok(SupplierCreationPlan {
        party,
        party_revision,
        supplier,
        commercial_profile,
        capabilities,
        capability_revisions,
        capability_ids,
        qualifications,
        qualification_revisions,
        qualification_links,
        rating,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PartyId;
    use std::str::FromStr;

    /// 构造最小合法创建输入。
    fn test_inputs() -> SupplierCreationInputs {
        SupplierCreationInputs {
            party_no: "PARTY-1".to_string(),
            supplier_no: "SUP-1".to_string(),
            legal_name: "示例企业".to_string(),
            short_name: None,
            unified_credit_code: None,
            settlement_mode: SettlementMode::Prepayment,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: "PREPAY_30".to_string(),
            business_category: None,
            invoice_type: InvoiceType::VatSpecial,
            invoice_tax_rate: Rate::from_str("0.13").unwrap(),
            signing_entity_party_id: PartyId::new("party-sign"),
            payment_entity_party_id: PartyId::new("party-pay"),
            capability_codes: vec![CapabilityCode::Physical],
            qualifications: vec![SupplierCreationQualificationInput {
                qualification_type: QualificationType::FoodLicense,
                certificate_no: "FOOD-1".to_string(),
                issuer: None,
                valid_from: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
                valid_to: None,
                attachment_id: None,
                capability_codes: vec![CapabilityCode::Physical],
            }],
            rating: Some(SupplierCreationRatingInput {
                initial_score: Some(80),
                rating: SupplierRating::A,
                current_score: 85,
                valid_from: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
            }),
            effective_from: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
            change_reason: "首次".to_string(),
            actor_id: "actor-1".to_string(),
        }
    }

    /// 构造与输入对应的已分配主键。
    fn test_ids(with_rating: bool) -> SupplierCreationIds {
        SupplierCreationIds {
            party_id: PartyId::new("party-1"),
            party_revision_id: crate::ids::PartyRevisionId::new("party-rev-1"),
            supplier_id: SupplierAccountId::new("supplier-1"),
            commercial_profile_id: crate::ids::SupplierCommercialProfileRevisionId::new("profile-1"),
            capability_ids: vec![(
                CapabilityCode::Physical,
                SupplierCapabilityId::new("cap-1"),
                SupplierCapabilityRevisionId::new("cap-rev-1"),
            )],
            qualification_ids: vec![SupplierCreationQualificationIds {
                qualification_id: SupplierQualificationId::new("qual-1"),
                revision_id: SupplierQualificationRevisionId::new("qual-rev-1"),
                link_ids: vec![crate::ids::SupplierQualificationCapabilityId::new("link-1")],
            }],
            rating_id: with_rating.then(|| SupplierRatingRevisionId::new("rating-1")),
        }
    }

    /// 首版修订号、当前指针、能力映射、资质关联与可选评级均合法生成。
    #[test]
    fn creation_plan_builds_first_revisions_and_links() {
        let plan = plan_supplier_creation(test_ids(true), test_inputs()).unwrap();
        assert_eq!(plan.party_revision.revision.revision_no, 1);
        assert_eq!(
            plan.party.stable.current_revision_id.as_deref(),
            Some(plan.party_revision.base.id.as_str())
        );
        assert_eq!(
            plan.supplier
                .current_commercial_profile_revision_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some(plan.commercial_profile.base.id.as_str())
        );
        assert_eq!(plan.commercial_profile.revision.revision_no, 1);
        assert_eq!(plan.capabilities.len(), 1);
        assert_eq!(plan.capability_revisions.len(), 1);
        assert_eq!(
            plan.capability_ids
                .get("physical")
                .map(ToString::to_string)
                .as_deref(),
            Some("cap-1")
        );
        assert_eq!(plan.qualifications.len(), 1);
        assert_eq!(plan.qualification_revisions.len(), 1);
        assert_eq!(plan.qualification_links.len(), 1);
        assert!(plan.rating.is_some());
        assert_eq!(plan.rating.as_ref().unwrap().revision.revision_no, 1);
    }

    /// 无评级输入时不生成评级实体。
    #[test]
    fn creation_plan_supports_missing_optional_rating() {
        let mut inputs = test_inputs();
        inputs.rating = None;
        let plan = plan_supplier_creation(test_ids(false), inputs).unwrap();
        assert!(plan.rating.is_none());
        assert_eq!(plan.qualifications.len(), 1);
    }

    /// 重复能力输入整体拒绝，不返回部分计划。
    #[test]
    fn creation_plan_rejects_duplicate_capabilities_atomically() {
        let mut inputs = test_inputs();
        inputs.capability_codes = vec![CapabilityCode::Physical, CapabilityCode::Physical];
        let ids = SupplierCreationIds {
            capability_ids: vec![
                (
                    CapabilityCode::Physical,
                    SupplierCapabilityId::new("cap-1"),
                    SupplierCapabilityRevisionId::new("cap-rev-1"),
                ),
                (
                    CapabilityCode::Physical,
                    SupplierCapabilityId::new("cap-2"),
                    SupplierCapabilityRevisionId::new("cap-rev-2"),
                ),
            ],
            ..test_ids(true)
        };
        assert!(plan_supplier_creation(ids, inputs).is_err());
    }

    /// 资质引用未勾选能力时整体拒绝。
    #[test]
    fn creation_plan_rejects_qualification_referencing_unselected_capability() {
        let mut inputs = test_inputs();
        inputs.qualifications[0].capability_codes = vec![CapabilityCode::Api];
        assert!(plan_supplier_creation(test_ids(true), inputs).is_err());
    }

    /// 能力 ID 数量与输入不一致时失败关闭。
    #[test]
    fn creation_plan_rejects_mismatched_allocated_ids() {
        let ids = SupplierCreationIds {
            capability_ids: vec![],
            ..test_ids(true)
        };
        assert!(plan_supplier_creation(ids, test_inputs()).is_err());
    }

    /// 评级输入与评级 ID 不一致时失败关闭。
    #[test]
    fn creation_plan_rejects_rating_input_id_mismatch() {
        let inputs = test_inputs();
        assert!(plan_supplier_creation(test_ids(false), inputs).is_err());
        let mut no_rating = test_inputs();
        no_rating.rating = None;
        assert!(plan_supplier_creation(test_ids(true), no_rating).is_err());
    }
}
