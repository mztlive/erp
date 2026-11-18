use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId, SupplierQualificationCapabilityId,
    SupplierQualificationId, SupplierQualificationRevisionId,
};

use super::types::option_as_authoritative_update;
use crate::entity::supplier::{
    CapabilityCode, CapabilityStatus, QualificationStatus, SupplierCapability, SupplierCapabilityData,
    SupplierCapabilityRevision, SupplierQualification, SupplierQualificationCapability,
    SupplierQualificationData, SupplierQualificationRevision, SupplierQualificationUpdate,
};

/// 用调用方分配的主键创建指定供应商的能力及首版修订，修订号为 1 并设置当前指针。负责人必须显式指定，创建
/// 人只用于审计，不能兜底；字段或修订构造失败时不返回结果。
pub fn new_capability(
    supplier_id: &SupplierAccountId,
    code: CapabilityCode,
    valid_from: BusinessDate,
    owner_user_id: &str,
    actor_id: &str,
    capability_id: SupplierCapabilityId,
    revision_id: SupplierCapabilityRevisionId,
) -> erp_core::Result<(SupplierCapability, SupplierCapabilityRevision)> {
    let mut capability = SupplierCapability::new(
        capability_id,
        SupplierCapabilityData {
            supplier_id: supplier_id.clone(),
            capability_code: code,
            service_region: None,
            owner_user_id: owner_user_id.to_string(),
            fulfillment_note: None,
            valid_from,
            valid_to: None,
            status: CapabilityStatus::Active,
        },
        actor_id,
    )?;

    let revision_key = revision_id.to_string();
    let revision = capability.snapshot_revision(revision_id, 1)?;
    capability.stable.current_revision_id = Some(revision_key);
    Ok((capability, revision))
}

/// 保存根资料时原地替换同一稳定资质的字段，缺省可空字段表示清空，并将停用或过期资质恢复启用；状态迁移不
/// 合法或生效区间冲突时返回错误。
pub fn apply_qualification_input(
    qualification: &mut SupplierQualification,
    issuer: Option<String>,
    valid_from: Option<BusinessDate>,
    valid_to: Option<BusinessDate>,
    attachment_id: Option<erp_core::ids::FileAssetId>,
    actor_id: &str,
) -> erp_core::Result<()> {
    let status = (!qualification.stable.status.is_valid()).then_some(QualificationStatus::Active);
    qualification.update(
        SupplierQualificationUpdate {
            issuer: option_as_authoritative_update(issuer),
            attachment_id: option_as_authoritative_update(attachment_id),
            valid_from: option_as_authoritative_update(valid_from),
            valid_to: option_as_authoritative_update(valid_to),
            status,
        },
        actor_id,
    )?;
    Ok(())
}

/// 创建新资质及关联的输入参数。
#[derive(Debug)]
pub struct NewQualificationParams<'a> {
    /// 供应商角色 ID。
    pub supplier_id: &'a SupplierAccountId,
    /// 资质类型。
    pub qualification_type: crate::entity::supplier::QualificationType,
    /// 证书编号。
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效日。
    pub valid_from: Option<BusinessDate>,
    /// 失效日。
    pub valid_to: Option<BusinessDate>,
    /// 附件。
    pub attachment_id: Option<erp_core::ids::FileAssetId>,
    /// 适用能力代码。
    pub capability_codes: &'a [CapabilityCode],
    /// 能力代码到稳定 ID 的映射。
    pub capability_ids: &'a std::collections::HashMap<String, SupplierCapabilityId>,
    /// 操作人 ID。
    pub actor_id: &'a str,
    /// 新资质主键。
    pub qualification_id: SupplierQualificationId,
    /// 首版修订主键。
    pub revision_id: SupplierQualificationRevisionId,
    /// 待创建关联主键列表，按 `capability_codes` 顺序一一对应。
    pub link_ids: Vec<SupplierQualificationCapabilityId>,
}

/// 用调用方分配的身份组装新资质、首版修订及适用能力关联；任一能力不在已确认映射中、关联 ID 数量不匹配或
/// 字段非法时拒绝，不查询数据库。
pub fn new_qualification(
    params: NewQualificationParams<'_>,
) -> erp_core::Result<(
    SupplierQualification,
    SupplierQualificationRevision,
    Vec<SupplierQualificationCapability>,
)> {
    let NewQualificationParams {
        supplier_id,
        qualification_type,
        certificate_no,
        issuer,
        valid_from,
        valid_to,
        attachment_id,
        capability_codes,
        capability_ids,
        actor_id,
        qualification_id,
        revision_id,
        link_ids,
    } = params;

    let mut qualification = SupplierQualification::new(
        qualification_id.clone(),
        SupplierQualificationData {
            supplier_id: supplier_id.clone(),
            qualification_type,
            certificate_no,
            issuer,
            valid_from,
            valid_to,
            attachment_id,
            status: QualificationStatus::Active,
        },
        actor_id,
    )?;
    qualification.stable.current_revision_id = Some(revision_id.to_string());
    let revision = SupplierQualification::snapshot_revision(&qualification, revision_id, 1)?;

    let links = SupplierQualificationCapability::links_for_qualification(
        qualification_id,
        capability_codes,
        capability_ids,
        link_ids,
    )?;
    Ok((qualification, revision, links))
}
