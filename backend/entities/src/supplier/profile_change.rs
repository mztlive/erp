//! 供应商资料根修订的领域变更计划（`PROC-E06`）。
//!
//! 集中资料修订中的状态迁移、旧默认事实停用、能力启停及资质字段变更等纯规则；
//! 不触及 MongoDB、HTTP、全局时钟或全局 ID，Service 显式注入已加载事实、
//! 新 ID、修订号、业务日与操作人。

use crate::common::time::BusinessDate;
use crate::field_update::FieldUpdate;
use crate::ids::{
    PartyId, PartyRevisionId, SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
    SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
    SupplierQualificationRevisionId,
};
use crate::party::{
    Party, PartyAddress, PartyAddressUpdate, PartyBankAccount, PartyBankAccountUpdate, PartyContact,
    PartyContactUpdate, PartyRevision, PartyRevisionData, PartyTaxProfile, PartyTaxProfileUpdate,
    PartyUpdate,
};
use crate::supplier::{
    qualification_identity_key, CapabilityCode, CapabilityStatus, InvoiceType, QualificationStatus,
    QualificationType, ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountUpdate,
    SupplierCapability, SupplierCapabilityData, SupplierCapabilityRevision,
    SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData, SupplierQualification,
    SupplierQualificationCapability, SupplierQualificationCapabilityData, SupplierQualificationData,
    SupplierQualificationRevision, SupplierQualificationUpdate,
};

/// 将可空输入映射为明确设置或清空意图。
///
/// # 参数
/// * `value` - 可空输入值
///
/// # 返回
/// `Some(v)` 映射为 `Set(v)`，`None` 映射为 `Clear`，调用方以 `Unchanged` 表达保留。
///
/// # 错误
/// 无；仅做枚举映射。
///
/// # 约束
/// 不触及 I/O，仅做纯映射；与 Service 侧 `option_as_authoritative_update` 保持等价。
fn option_as_authoritative_update<T>(value: Option<T>) -> FieldUpdate<T> {
    value.map_or(FieldUpdate::Clear, FieldUpdate::Set)
}

/// 创建主体名称新修订并更新统一社会信用代码。
///
/// # 参数
/// * `party` - 待修订的主体实体，已通过版本与启停门禁；方法内原地更新 `unified_credit_code` 并推进 `current_revision_id`
/// * `unified_credit_code` - 统一社会信用代码输入，`Some` 表示设置、`None` 表示清空
/// * `legal_name` - 法定名称
/// * `short_name` - 简称
/// * `change_reason` - 修订原因
/// * `revision_id` - 新修订主键，由 Service 分配
/// * `revision_no` - 新修订序号，由 Service 查询得出
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 返回新建的 `PartyRevision`，并已推进 `party.current_revision_id`。
///
/// # 错误
/// 统一社会信用代码格式非法、法定名称校验失败或修订号越界时返回错误。
///
/// # 约束
/// 纯内存操作，不触及 MongoDB、全局 ID 或加密；`party_no` 与 `party_kind` 不在此修改。
#[allow(clippy::too_many_arguments)]
pub fn plan_party_revision(
    party: &mut Party,
    unified_credit_code: Option<String>,
    legal_name: String,
    short_name: Option<String>,
    change_reason: String,
    revision_id: PartyRevisionId,
    revision_no: u32,
    actor_id: &str,
) -> crate::Result<PartyRevision> {
    party.update(
        PartyUpdate {
            unified_credit_code: option_as_authoritative_update(unified_credit_code),
            status: None,
        },
        actor_id,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    PartyRevision::new(
        revision_id,
        PartyRevisionData {
            party_id: PartyId::new(&party.base.id),
            revision_no,
            legal_name,
            short_name,
            change_reason,
        },
    )
}

/// 创建商务资料新修订并推进供应商当前指针。
///
/// # 参数
/// * `supplier` - 待修订的供应商实体，已通过版本门禁；原地推进 `current_commercial_profile_revision_id`
/// * `settlement_mode` - 结算方式
/// * `reconciliation_cycle` - 对账周期
/// * `payment_term_snapshot` - 付款条件快照
/// * `business_category` - 经营类目
/// * `invoice_type` - 发票类型
/// * `invoice_tax_rate` - 发票税点
/// * `signing_entity_party_id` - 签约主体
/// * `payment_entity_party_id` - 付款主体
/// * `change_reason` - 变更原因
/// * `revision_id` - 新修订主键
/// * `revision_no` - 新修订序号
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 返回新建的商务资料修订，并已推进 `supplier` 指针。
///
/// # 错误
/// 付款条件、税点或变更原因非法时返回错误。
///
/// # 约束
/// 纯内存，不触及外部 I/O；不分配新 ID，需 Service 注入。
#[allow(clippy::too_many_arguments)]
pub fn plan_commercial_profile_revision(
    supplier: &mut SupplierAccount,
    settlement_mode: SettlementMode,
    reconciliation_cycle: ReconciliationCycle,
    payment_term_snapshot: String,
    business_category: Option<String>,
    invoice_type: InvoiceType,
    invoice_tax_rate: crate::money::Rate,
    signing_entity_party_id: PartyId,
    payment_entity_party_id: PartyId,
    change_reason: String,
    revision_id: SupplierCommercialProfileRevisionId,
    revision_no: u32,
    actor_id: &str,
) -> crate::Result<SupplierCommercialProfileRevision> {
    let revision = SupplierCommercialProfileRevision::new(
        revision_id.clone(),
        SupplierCommercialProfileRevisionData {
            supplier_id: SupplierAccountId::new(&supplier.base.id),
            revision_no,
            settlement_mode,
            reconciliation_cycle,
            payment_term_snapshot,
            business_category,
            invoice_type,
            invoice_tax_rate,
            signing_entity_party_id,
            payment_entity_party_id,
            change_reason,
        },
    )?;
    supplier.update(
        SupplierAccountUpdate {
            default_payment_term_id: FieldUpdate::Unchanged,
            current_commercial_profile_revision_id: FieldUpdate::Set(revision_id),
            status: None,
        },
        actor_id,
    )?;
    Ok(revision)
}

/// 停用既有联系人事实行，供新默认事实行接替。
///
/// # 参数
/// * `items` - 已加载的联系人集合，已按仓储返回顺序传入
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 仅保留原 `Active` 记录并将其置为 `Disabled` 且 `is_default=false`；`Inactive` 已被过滤视为不存在。
///
/// # 错误
/// 状态迁移非法时返回错误；区间倒挂不在本方法校验。
///
/// # 约束
/// 纯内存；保留 Service 侧 `retain(is_active)` 语义，不得改变过滤顺序。
pub fn disable_contacts(items: &mut Vec<PartyContact>, actor_id: &str) -> crate::Result<()> {
    items.retain(PartyContact::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyContactUpdate {
                status: Some(crate::party::status::EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有地址事实行。
///
/// # 参数
/// * `items` - 已加载的地址集合
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 仅对 `Active` 记录执行停用，其余过滤。
///
/// # 错误
/// 状态迁移非法时返回错误。
///
/// # 约束
/// 纯内存；与 `disable_contacts` 同构，保持一致的停用语义。
pub fn disable_addresses(items: &mut Vec<PartyAddress>, actor_id: &str) -> crate::Result<()> {
    items.retain(PartyAddress::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyAddressUpdate {
                status: Some(crate::party::status::EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有税务事实行。
///
/// # 参数
/// * `items` - 已加载的税务档案集合
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 仅 `Active` 被停用。
///
/// # 错误
/// 状态迁移非法时返回错误。
///
/// # 约束
/// 纯内存；保持原 Service 顺序。
pub fn disable_tax_profiles(items: &mut Vec<PartyTaxProfile>, actor_id: &str) -> crate::Result<()> {
    items.retain(PartyTaxProfile::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyTaxProfileUpdate {
                status: Some(crate::party::status::EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有银行账户事实行。
///
/// # 参数
/// * `items` - 已加载的银行账户集合
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 仅 `Active` 被停用。
///
/// # 错误
/// 状态迁移非法时返回错误。
///
/// # 约束
/// 纯内存；不触及加密列，仅切换状态与默认值。
pub fn disable_bank_accounts(items: &mut Vec<PartyBankAccount>, actor_id: &str) -> crate::Result<()> {
    items.retain(PartyBankAccount::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyBankAccountUpdate {
                status: Some(crate::party::status::EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

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

/// 能力变更中需切换状态的既有能力。
///
/// # 约束
/// 仅表达目标状态，不触及持久化；调用方负责生成修订与快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityToggle {
    /// 既有能力稳定 ID。
    pub capability_id: SupplierCapabilityId,
    /// 能力代码。
    pub code: CapabilityCode,
    /// 切换后目标状态。
    pub target_status: CapabilityStatus,
}

/// 根资料资质在领域层的最小输入视图，用于变更计划计算。
///
/// # 约束
/// 仅携带参与 `matches_profile_fields` 与关联集合比对的字段；不含文件资产解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedQualificationInput {
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书编号原始输入。
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效日期。
    pub valid_from: BusinessDate,
    /// 失效日期。
    pub valid_to: Option<BusinessDate>,
    /// 资质附件 ID。
    pub attachment_id: Option<crate::ids::FileAssetId>,
    /// 适用能力代码集合。
    pub capability_codes: Vec<CapabilityCode>,
}

/// 供应商资料根修订的领域变更计划。
///
/// 聚合能力启停与资质字段/关联差异的纯业务决策；不触及 MongoDB、时钟或 ID 生成。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierProfileChangePlan {
    /// 需切换状态的既有能力。
    pub capability_toggles: Vec<CapabilityToggle>,
    /// 需新建的能力代码。
    pub capability_creates: Vec<CapabilityCode>,
    /// 需更新字段或关联的既有资质稳定身份键。
    pub qualification_updates: Vec<String>,
    /// 需停用的既有资质稳定身份键。
    pub qualification_disables: Vec<String>,
    /// 需新建的资质输入。
    pub qualification_creates: Vec<PlannedQualificationInput>,
}

impl SupplierProfileChangePlan {
    /// 从已加载事实与根资料请求计算变更计划。
    ///
    /// # 参数
    /// * `capabilities` - 已加载的供应商既有能力集合，按仓储返回顺序传入
    /// * `qualifications` - 已加载的供应商既有资质集合
    /// * `linked_capabilities` - 资质 ID 到适用能力 ID 集合的映射，由仓储批量读取
    /// * `capability_ids` - 请求能力代码到稳定能力 ID 的映射，用于资质关联一致性校验
    /// * `requested_capability_codes` - 根资料请求中的能力代码集合
    /// * `requested_qualifications` - 根资料请求中的资质输入视图
    ///
    /// # 返回
    /// 返回仅含需变更项的精简计划；无变化时对应向量为空。
    ///
    /// # 错误
    /// 资质适用能力不存在或关联不一致时返回校验错误。
    ///
    /// # 约束
    /// 纯内存计算，不触及 MongoDB、全局 ID 或时钟；判定逻辑与 `profile.rs` 原 Service
    /// helper 完全一致（`wanted == is_active` 能力跳过，`matches_profile_fields` 与
    /// `current_links == desired_links` 资质跳过），便于单测锁定。
    #[allow(clippy::too_many_arguments)]
    pub fn from_loaded(
        capabilities: &[SupplierCapability],
        qualifications: &[SupplierQualification],
        linked_capabilities: &std::collections::HashMap<String, std::collections::HashSet<String>>,
        capability_ids: &std::collections::HashMap<String, SupplierCapabilityId>,
        requested_capability_codes: &[CapabilityCode],
        requested_qualifications: &[PlannedQualificationInput],
    ) -> crate::Result<Self> {
        use std::collections::{HashMap, HashSet};
        let requested_set: HashSet<String> = requested_capability_codes
            .iter()
            .map(|code| code.as_str().to_string())
            .collect();
        let capability_index: HashMap<String, &SupplierCapability> = capabilities
            .iter()
            .map(|cap| (cap.capability_code.as_str().to_string(), cap))
            .collect();
        let mut capability_toggles = Vec::new();
        for cap in capabilities {
            let wanted = requested_set.contains(cap.capability_code.as_str());
            if wanted == cap.is_active() {
                continue;
            }
            let target_status = if wanted {
                CapabilityStatus::Active
            } else {
                CapabilityStatus::Disabled
            };
            capability_toggles.push(CapabilityToggle {
                capability_id: SupplierCapabilityId::new(&cap.base.id),
                code: cap.capability_code,
                target_status,
            });
        }
        let mut capability_creates = Vec::new();
        for code in requested_capability_codes {
            if !capability_index.contains_key(code.as_str()) && !capability_creates.contains(code) {
                capability_creates.push(*code);
            }
        }
        let requested_map: HashMap<String, &PlannedQualificationInput> = requested_qualifications
            .iter()
            .map(|input| {
                (
                    qualification_identity_key(input.qualification_type, &input.certificate_no),
                    input,
                )
            })
            .collect();
        let existing_keys: HashSet<String> = qualifications.iter().map(|q| q.identity_key()).collect();
        let mut qualification_updates = Vec::new();
        let mut qualification_disables = Vec::new();
        for qual in qualifications {
            let key = qual.identity_key();
            if let Some(input) = requested_map.get(&key) {
                let desired_links: HashSet<String> = input
                    .capability_codes
                    .iter()
                    .map(|code| {
                        capability_ids
                            .get(code.as_str())
                            .map(ToString::to_string)
                            .ok_or_else(|| crate::Error::from("资质适用能力不存在"))
                    })
                    .collect::<crate::Result<_>>()?;
                let current_links = linked_capabilities
                    .get(&qual.base.id)
                    .cloned()
                    .unwrap_or_default();
                if qual.matches_profile_fields(
                    input.issuer.as_deref(),
                    input.valid_from,
                    input.valid_to,
                    input.attachment_id.as_ref(),
                ) && current_links == desired_links
                {
                    continue;
                }
                qualification_updates.push(key);
            } else if qual.is_valid() {
                qualification_disables.push(key);
            }
        }
        let mut qualification_creates = Vec::new();
        for input in requested_qualifications {
            let key = qualification_identity_key(input.qualification_type, &input.certificate_no);
            if !existing_keys.contains(&key) {
                qualification_creates.push(input.clone());
            }
        }
        Ok(Self {
            capability_toggles,
            capability_creates,
            qualification_updates,
            qualification_disables,
            qualification_creates,
        })
    }
}

#[cfg(test)]
mod tests {
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
        PartyBankAccountData, PartyContact, PartyContactData, PartyData, PartyKind, PartyStatus,
        PartyTaxProfile, PartyTaxProfileData,
    };
    use crate::supplier::{
        CapabilityCode, InvoiceType, QualificationStatus, QualificationType, ReconciliationCycle,
        SettlementMode, SupplierAccount, SupplierAccountData, SupplierAccountStatus, SupplierQualification,
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
            SupplierAccountId, SupplierCapabilityId, SupplierQualificationCapabilityId,
            SupplierQualificationId, SupplierQualificationRevisionId,
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
}
