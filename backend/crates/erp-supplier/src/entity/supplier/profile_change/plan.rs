use crate::entity::supplier::{
    qualification_identity_key, CapabilityCode, CapabilityStatus, QualificationStatus, QualificationType,
    SupplierCapability, SupplierQualification,
};
use erp_core::common::time::BusinessDate;
use erp_core::ids::SupplierCapabilityId;

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
    pub valid_from: Option<BusinessDate>,
    /// 失效日期。
    pub valid_to: Option<BusinessDate>,
    /// 资质附件 ID。
    pub attachment_id: Option<erp_core::ids::FileAssetId>,
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
    pub fn from_loaded(
        capabilities: &[SupplierCapability],
        qualifications: &[SupplierQualification],
        linked_capabilities: &std::collections::HashMap<String, std::collections::HashSet<String>>,
        capability_ids: &std::collections::HashMap<String, SupplierCapabilityId>,
        requested_capability_codes: &[CapabilityCode],
        requested_qualifications: &[PlannedQualificationInput],
    ) -> erp_core::Result<Self> {
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
                            .ok_or_else(|| erp_core::Error::from("资质适用能力不存在"))
                    })
                    .collect::<erp_core::Result<_>>()?;
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
            } else if qual.stable.status == QualificationStatus::Active {
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
