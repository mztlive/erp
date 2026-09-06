use crate::common::time::BusinessDate;
use crate::ids::{
    SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId, SupplierQualificationCapabilityId,
    SupplierQualificationId, SupplierQualificationRevisionId,
};
use crate::supplier::{
    CapabilityCode, CapabilityStatus, QualificationStatus, SupplierCapability, SupplierCapabilityData,
    SupplierCapabilityRevision, SupplierQualification, SupplierQualificationCapability,
    SupplierQualificationCapabilityData, SupplierQualificationData, SupplierQualificationRevision,
    SupplierQualificationUpdate,
};

use super::types::option_as_authoritative_update;

/// 创建一项新能力及首版快照。
///
/// # 参数
/// * `supplier_id` - 供应商角色 ID
/// * `code` - 能力代码
/// * `valid_from` - 生效起始日
/// * `actor_id` - 操作人 ID
/// * `capability_id` - 新能力主键，Service 分配
/// * `revision_id` - 首版修订主键，Service 分配
///
/// # 返回
/// 返回 `(Capability, Revision)`，修订号固定为 `1` 且 `current_revision_id` 已推进。
///
/// # 错误
/// 能力字段校验失败或修订创建失败时返回错误。
///
/// # 约束
/// 纯内存，不生成 ID，不查询 DB，修订号不做溢出判断（首版恒为 1）；修订快照
/// 通过 `SupplierCapability::snapshot_revision` 生成，字段与实体当前状态逐字段一致。
pub fn new_capability(
    supplier_id: &SupplierAccountId,
    code: CapabilityCode,
    valid_from: BusinessDate,
    actor_id: &str,
    capability_id: SupplierCapabilityId,
    revision_id: SupplierCapabilityRevisionId,
) -> crate::Result<(SupplierCapability, SupplierCapabilityRevision)> {
    let mut capability = SupplierCapability::new(
        capability_id,
        SupplierCapabilityData {
            supplier_id: supplier_id.clone(),
            capability_code: code,
            service_region: None,
            owner_user_id: actor_id.to_string(),
            fulfillment_note: None,
            valid_from,
            valid_to: None,
            status: CapabilityStatus::Active,
        },
        actor_id,
    )?;
    let revision = capability.snapshot_revision(revision_id.clone(), 1)?;
    capability.stable.current_revision_id = Some(revision_id.to_string());
    Ok((capability, revision))
}

/// 将根命令资质字段应用到同一稳定资质。
///
/// # 参数
/// * `qualification` - 待更新的资质实体；若当前为 `Disabled/Expired` 则自动切回 `Active`
/// * `issuer` - 发证机构输入，`Some` 设置、`None` 清空
/// * `valid_from` - 生效起始日输入
/// * `valid_to` - 失效日输入，`Some` 设置、`None` 清空
/// * `attachment_id` - 附件输入
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 原地更新资质实体。
///
/// # 错误
/// 资质状态迁移或区间校验失败时返回错误。
///
/// # 约束
/// `valid_from` 按完全替换语义写入；`status` 仅在非 Active 时自动置 Active，保持与旧 Service 一致。
pub fn apply_qualification_input(
    qualification: &mut SupplierQualification,
    issuer: Option<String>,
    valid_from: BusinessDate,
    valid_to: Option<BusinessDate>,
    attachment_id: Option<crate::ids::FileAssetId>,
    actor_id: &str,
) -> crate::Result<()> {
    let status = (!qualification.is_valid()).then_some(QualificationStatus::Active);
    qualification.update(
        SupplierQualificationUpdate {
            issuer: option_as_authoritative_update(issuer),
            attachment_id: option_as_authoritative_update(attachment_id),
            valid_from: Some(valid_from),
            valid_to: option_as_authoritative_update(valid_to),
            status,
        },
        actor_id,
    )?;
    Ok(())
}

/// 创建一份新资质、首版快照及适用能力关联的领域组装数据。
///
/// # 参数
/// * `supplier_id` - 供应商角色 ID
/// * `qualification_type` - 资质类型
/// * `certificate_no` - 证书编号
/// * `issuer` - 发证机构
/// * `valid_from` - 生效日
/// * `valid_to` - 失效日
/// * `attachment_id` - 附件
/// * `capability_codes` - 适用能力代码
/// * `capability_ids` - 能力代码到稳定 ID 的映射
/// * `actor_id` - 操作人 ID
/// * `qualification_id` - 新资质主键
/// * `revision_id` - 首版修订主键
/// * `link_ids` - 待创建关联主键列表，按 `capability_codes` 顺序一一对应
///
/// # 返回
/// 返回 `(Qualification, Revision, Links)`，修订号固定为 1。
///
/// # 错误
/// 任一能力码未在 `capability_ids` 中、字段校验或关联构造失败时返回错误；`link_ids` 长度与 `capability_codes` 不一致时也返回错误。
///
/// # 约束
/// 纯内存，不触及 DB；`supplier_id` 与 `capability_ids` 由 Service 保证为当前有效能力。
#[allow(clippy::too_many_arguments)]
pub fn new_qualification(
    supplier_id: &SupplierAccountId,
    qualification_type: crate::supplier::QualificationType,
    certificate_no: String,
    issuer: Option<String>,
    valid_from: BusinessDate,
    valid_to: Option<BusinessDate>,
    attachment_id: Option<crate::ids::FileAssetId>,
    capability_codes: &[CapabilityCode],
    capability_ids: &std::collections::HashMap<String, SupplierCapabilityId>,
    actor_id: &str,
    qualification_id: SupplierQualificationId,
    revision_id: SupplierQualificationRevisionId,
    link_ids: Vec<SupplierQualificationCapabilityId>,
) -> crate::Result<(
    SupplierQualification,
    SupplierQualificationRevision,
    Vec<SupplierQualificationCapability>,
)> {
    if capability_codes.len() != link_ids.len() {
        return Err(crate::Error::from("资质适用能力与关联 ID 数量不一致"));
    }
    let mut qualification = SupplierQualification::new(
        qualification_id.clone(),
        SupplierQualificationData {
            supplier_id: supplier_id.clone(),
            qualification_type,
            certificate_no: certificate_no.clone(),
            issuer: issuer.clone(),
            valid_from,
            valid_to,
            attachment_id: attachment_id.clone(),
            status: QualificationStatus::Active,
        },
        actor_id,
    )?;
    qualification.stable.current_revision_id = Some(revision_id.to_string());
    let revision = SupplierQualification::snapshot_revision(&qualification, revision_id, 1)?;
    let mut links = Vec::with_capacity(capability_codes.len());
    for (code, link_id) in capability_codes.iter().zip(link_ids) {
        let capability_id = capability_ids
            .get(code.as_str())
            .ok_or_else(|| crate::Error::from("资质适用能力不存在"))?;
        links.push(SupplierQualificationCapability::new(
            link_id,
            SupplierQualificationCapabilityData {
                qualification_id: qualification_id.clone(),
                capability_id: capability_id.clone(),
            },
        )?);
    }
    Ok((qualification, revision, links))
}
