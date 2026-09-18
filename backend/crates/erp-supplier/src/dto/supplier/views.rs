use std::collections::HashMap;

use erp_core::money::Rate;
use serde::Serialize;

use super::list::SupplierQualificationHealth;
use super::reveal::SupplierSensitiveFieldView;
use crate::entity::supplier::{
    CapabilityCode, CapabilityStatus, InvoiceType, QualificationStatus, QualificationType,
    ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountStatus, SupplierCapability,
    SupplierCapabilityId, SupplierCommercialProfileRevision, SupplierQualification, SupplierRating,
    SupplierRatingRevision,
};
use crate::ports::{PartyAddressFact, PartyBankAccountFact, PartyContactFact, PartyTaxProfileFact};

/// 供应商角色响应视图（列表用，契约形状对齐 `supplier_account` 投影行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierView {
    /// 实体主键。
    pub id: String,
    /// 共用企业主体 ID。
    pub party_id: String,
    /// 企业主体编号；列表由服务端批量投影填充。
    pub party_no: Option<String>,
    /// 当前法定名称；列表由服务端批量投影填充。
    pub legal_name: Option<String>,
    /// 当前简称；列表由服务端批量投影填充。
    pub short_name: Option<String>,
    /// 主体乐观锁版本。
    pub party_version: Option<u64>,
    /// 供应商编号。
    pub supplier_no: String,
    /// 默认结算条件引用。
    pub default_payment_term_id: Option<String>,
    /// 当前商务结算版本 ID。
    pub current_commercial_profile_revision_id: Option<String>,
    /// 启停状态。
    pub status: SupplierAccountStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 整体维护人。
    #[serde(default)]
    pub maintainer_user_id: String,
    /// 整体维护人显示名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintainer_user_name: Option<String>,
    /// 当前业务组织。
    #[serde(default)]
    pub business_org_unit_id: String,
    /// 当前商务资料；列表由服务端批量投影填充。
    pub current_profile: Option<CommercialProfileView>,
    /// 当前有效供应能力代码；未水合时为空。
    #[serde(default)]
    pub capability_codes: Vec<CapabilityCode>,
    /// 列表折叠后的资质健康状态；未水合时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualification_health: Option<SupplierQualificationHealth>,
    /// 已登记资质类型；未水合时为空。
    #[serde(default)]
    pub qualification_types: Vec<QualificationType>,
}

impl From<SupplierAccount> for SupplierView {
    /// 从实体构造响应视图。
    fn from(account: SupplierAccount) -> Self {
        Self {
            id: account.base.id,
            party_id: account.party_id.to_string(),
            party_no: None,
            legal_name: None,
            short_name: None,
            party_version: None,
            supplier_no: account.supplier_no,
            default_payment_term_id: account.default_payment_term_id,
            current_commercial_profile_revision_id: account
                .current_commercial_profile_revision_id
                .map(|id| id.to_string()),
            status: account.stable.status,
            version: account.base.version,
            created_at: account.base.created_at,
            maintainer_user_id: account.maintainer_user_id,
            maintainer_user_name: None,
            business_org_unit_id: account.business_org_unit_id,
            current_profile: None,
            capability_codes: Vec::new(),
            qualification_health: None,
            qualification_types: Vec::new(),
        }
    }
}

/// 供应商角色详情视图；当前主体名称与商务资料已由账户投影内联。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierDetailView {
    /// 供应商角色响应视图。
    #[serde(flatten)]
    pub account: SupplierView,
    /// 主体状态。
    pub party_status: crate::ports::PartyStatusFact,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 联系人事实行。
    pub contacts: Vec<PartyContactFact>,
    /// 地址事实行（地址正文通过敏感字段揭示接口读取）。
    pub addresses: Vec<PartyAddressFact>,
    /// 税务事实行。
    pub tax_profiles: Vec<PartyTaxProfileFact>,
    /// 银行账户摘要；不返回账号明文。
    pub bank_accounts: Vec<PartyBankAccountFact>,
    /// 供应商能力。
    pub capabilities: Vec<SupplierCapabilityView>,
    /// 供应商资质及其适用能力。
    pub qualifications: Vec<SupplierQualificationView>,
    /// 供应商评级历史。
    pub ratings: Vec<SupplierRatingView>,
    /// 商务资料历史。
    pub commercial_profiles: Vec<CommercialProfileView>,
    /// 当前默认敏感字段的掩码与短时揭示令牌。
    pub sensitive_fields: Vec<SupplierSensitiveFieldView>,
}
/// 商务结算版本响应视图（契约形状对齐投影行）。
///
/// 列表与详情均由当前商务资料实体映射；签约/付款主体名称由 Service 按主体
/// ID 批量回填，缺失当前名称时保持 `None`。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommercialProfileView {
    /// 实体主键。
    pub id: String,
    /// 供应商角色 ID。
    pub supplier_id: String,
    /// 商务版本号。
    pub revision_no: u32,
    /// 结算方式。
    pub settlement_mode: SettlementMode,
    /// 对账周期。
    pub reconciliation_cycle: ReconciliationCycle,
    /// 付款条件快照（不含经营类目编码）。
    pub payment_term_snapshot: String,
    /// 经营类目；未登记时为空。
    pub business_category: Option<String>,
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点（详情返回，列表为 `None`）。
    pub invoice_tax_rate: Option<Rate>,
    /// 常用进项税率；None 读取旧单值，Some([]) 明确表示未登记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoice_tax_rates: Option<Vec<Rate>>,
    /// 签约主体（详情返回，列表为 `None`）。
    pub signing_entity_party_id: Option<String>,
    /// 签约主体当前法定名称。
    pub signing_entity_name: Option<String>,
    /// 付款主体（详情返回，列表为 `None`）。
    pub payment_entity_party_id: Option<String>,
    /// 付款主体当前法定名称。
    pub payment_entity_name: Option<String>,
    /// 变更原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<SupplierCommercialProfileRevision> for CommercialProfileView {
    /// 从实体构造响应视图。
    fn from(revision: SupplierCommercialProfileRevision) -> Self {
        let payment_term_snapshot = revision.effective_payment_term_code();
        let business_category = revision.effective_business_category();
        Self {
            id: revision.base.id,
            supplier_id: revision.supplier_id.to_string(),
            revision_no: revision.revision.revision_no,
            settlement_mode: revision.settlement_mode,
            reconciliation_cycle: revision.reconciliation_cycle,
            payment_term_snapshot,
            business_category,
            invoice_type: revision.invoice_type,
            invoice_tax_rate: revision.invoice_tax_rate,
            invoice_tax_rates: revision.invoice_tax_rates.clone(),
            signing_entity_party_id: Some(revision.signing_entity_party_id.to_string()),
            signing_entity_name: None,
            payment_entity_party_id: Some(revision.payment_entity_party_id.to_string()),
            payment_entity_name: None,
            change_reason: revision.change_reason,
            version: revision.base.version,
            created_at: revision.base.created_at,
        }
    }
}

impl CommercialProfileView {
    /// 回填签约/付款主体的当前法定名称。
    ///
    /// 列表装配与详情装配共用本实现；缺失当前名称时保持 `None`。
    ///
    /// # 参数
    /// * `names` - 主体 ID 到法定名称
    ///
    /// # 返回
    /// 无返回值；视图在原地更新。
    pub(crate) fn fill_entity_names(&mut self, names: &HashMap<String, String>) {
        self.signing_entity_name =
            self.signing_entity_party_id.as_ref().and_then(|party_id| names.get(party_id)).cloned();
        self.payment_entity_name =
            self.payment_entity_party_id.as_ref().and_then(|party_id| names.get(party_id)).cloned();
    }
}

/// 供应商能力响应视图（契约形状对齐投影行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierCapabilityView {
    /// 实体主键。
    pub id: String,
    /// 供应商角色 ID。
    pub supplier_id: String,
    /// 能力代码。
    pub capability_code: CapabilityCode,
    /// 服务区域。
    pub service_region: Option<String>,
    /// 负责人。
    pub owner_user_id: String,
    /// 履约说明。
    pub fulfillment_note: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 启停状态。
    pub status: CapabilityStatus,
    /// 当前不可变能力修订。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<SupplierCapability> for SupplierCapabilityView {
    /// 从实体构造响应视图。
    fn from(capability: SupplierCapability) -> Self {
        Self {
            id: capability.base.id,
            supplier_id: capability.supplier_id.to_string(),
            capability_code: capability.capability_code,
            service_region: capability.service_region,
            owner_user_id: capability.owner_user_id,
            fulfillment_note: capability.fulfillment_note,
            valid_from: capability.valid_from.to_string(),
            valid_to: capability.valid_to.map(|date| date.to_string()),
            status: capability.stable.status,
            current_revision_id: capability.stable.current_revision_id,
            version: capability.base.version,
            created_at: capability.base.created_at,
        }
    }
}

/// 供应商资质响应视图（契约形状对齐投影行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierQualificationView {
    /// 实体主键。
    pub id: String,
    /// 供应商角色 ID。
    pub supplier_id: String,
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书编号。
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效、失效日期。
    pub valid_from: Option<String>,
    /// 失效日期。
    pub valid_to: Option<String>,
    /// 资质附件 ID。
    pub attachment_id: Option<String>,
    /// 资质状态。
    pub status: QualificationStatus,
    /// 适用能力 ID 集合。
    pub capability_ids: Vec<SupplierCapabilityId>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 合同起止日期完整后才可认定有效期已核实。
    pub validity_verified: bool,
}

impl From<SupplierQualification> for SupplierQualificationView {
    /// 从实体构造响应视图。
    fn from(qualification: SupplierQualification) -> Self {
        let validity_verified = qualification.validity_verified();
        Self {
            validity_verified,
            id: qualification.base.id,
            supplier_id: qualification.supplier_id.to_string(),
            qualification_type: qualification.qualification_type,
            certificate_no: qualification.certificate_no,
            issuer: qualification.issuer,
            valid_from: qualification.valid_from.map(|date| date.to_string()),
            valid_to: qualification.valid_to.map(|date| date.to_string()),
            attachment_id: qualification.attachment_id.map(|id| id.to_string()),
            status: qualification.stable.status,
            capability_ids: Vec::new(),
            version: qualification.base.version,
            created_at: qualification.base.created_at,
        }
    }
}

/// 供应商评估版本响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierRatingView {
    /// 实体主键。
    pub id: String,
    /// 供应商角色 ID。
    pub supplier_id: String,
    /// 评估版本号。
    pub revision_no: u32,
    /// 合作期初评分。
    pub initial_score: Option<u8>,
    /// 供应商评级。
    pub rating: SupplierRating,
    /// 合作中评分。
    pub current_score: u8,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 变更原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<SupplierRatingRevision> for SupplierRatingView {
    /// 从实体构造响应视图。
    fn from(revision: SupplierRatingRevision) -> Self {
        Self {
            id: revision.base.id,
            supplier_id: revision.supplier_id.to_string(),
            revision_no: revision.revision.revision_no,
            initial_score: revision.initial_score,
            rating: revision.rating,
            current_score: revision.current_score,
            valid_from: revision.valid_from.to_string(),
            valid_to: revision.valid_to.map(|date| date.to_string()),
            change_reason: revision.change_reason,
            version: revision.base.version,
            created_at: revision.base.created_at,
        }
    }
}
