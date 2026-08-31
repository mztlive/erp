//! 域 D09 `supplier` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；业务日期一律 `YYYY-MM-DD`；时间一律秒级时间戳；
//! 金额/税率按 P0 约定序列化为字符串（`invoice_tax_rate` 为 `Rate` 定点小数）。

use entities::common::time::BusinessDate;
use entities::ids::{FileAssetId, SupplierCapabilityId};
use entities::money::Rate;
use entities::supplier::{
    CapabilityCode, CapabilityStatus, InvoiceType, QualificationStatus, QualificationType,
    ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountStatus, SupplierCapability,
    SupplierCommercialProfileRevision, SupplierQualification, SupplierRating, SupplierRatingRevision,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::party::{PartyAddressView, PartyBankAccountView, PartyContactView, PartyTaxProfileView};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 供应商角色列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const SUPPLIER_SORT_FIELDS: &[&str] = &["created_at", "supplier_no", "status"];

/// 排序方向。
pub use crate::query::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空。
use crate::query::non_blank;

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
    /// 当前商务资料；列表由服务端批量投影填充。
    pub current_profile: Option<CommercialProfileView>,
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
            current_profile: None,
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
    pub party_status: entities::party::PartyStatus,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 联系人事实行。
    pub contacts: Vec<PartyContactView>,
    /// 地址事实行（地址正文通过敏感字段揭示接口读取）。
    pub addresses: Vec<PartyAddressView>,
    /// 税务事实行。
    pub tax_profiles: Vec<PartyTaxProfileView>,
    /// 银行账户摘要；不返回账号明文。
    pub bank_accounts: Vec<PartyBankAccountView>,
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

/// 供应商详情中的单个敏感字段揭示入口。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSensitiveFieldView {
    /// 页面稳定标签。
    pub label: String,
    /// 掩码展示值。
    pub masked_value: String,
    /// 受字段、事实行和供应商约束的短时令牌。
    pub reveal_token: String,
    /// 令牌过期时间（Unix 秒）。
    pub expires_at: u64,
}

/// 敏感字段揭示请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RevealSupplierSensitiveRequest {
    /// 详情接口签发的短时令牌。
    #[validate(custom(function = "non_blank", message = "揭示令牌不能为空"))]
    pub reveal_token: String,
}

/// 敏感字段揭示结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSensitiveRevealView {
    /// 解密后的明文；仅返回给已通过专用权限校验的当前请求。
    pub value: String,
}

/// 供应商角色列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierListParams {
    /// 供应商编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体 ID（精确匹配）。
    pub party_id: Option<entities::ids::PartyId>,
    /// 启停状态筛选。
    pub status: Option<SupplierAccountStatus>,
    /// 供应能力代码，多项以逗号分隔；命中任一当前有效能力即可。
    pub capability_codes: Option<String>,
    /// 资质类型代码，多项以逗号分隔；命中任一类型即可。
    pub qualification_types: Option<String>,
    /// 资质资料状态筛选。
    pub qualification_health: Option<SupplierQualificationHealth>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`supplier_no`/`status`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的供应商角色列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupplierListQuery {
    /// 供应商编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体 ID。
    pub party_id: Option<entities::ids::PartyId>,
    /// 启停状态筛选。
    pub status: Option<SupplierAccountStatus>,
    /// 命中任一当前有效能力的能力代码。
    pub capability_codes: Vec<CapabilityCode>,
    /// 命中任一资质类型的类型代码。
    pub qualification_types: Vec<QualificationType>,
    /// 资质资料状态筛选。
    pub qualification_health: Option<SupplierQualificationHealth>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierListParams {
    /// 归一化供应商角色列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SupplierListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, SUPPLIER_SORT_FIELDS)?;
        Ok(SupplierListQuery {
            keyword: normalized_text(self.keyword.as_deref()),
            party_id: self.party_id.clone(),
            status: self.status,
            capability_codes: normalize_capability_codes(self.capability_codes.as_deref())?,
            qualification_types: normalize_qualification_types(self.qualification_types.as_deref())?,
            qualification_health: self.qualification_health,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 供应商资质资料的当前状态。
///
/// 状态以当前业务日、资质启停状态和有效期共同计算；`NotRegistered` 仅表示
/// 尚未登记对应资质记录，不推断业务规则中的必备资质。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierQualificationHealth {
    /// 当前有效。
    Valid,
    /// 当前有效，且将在 30 天内到期。
    Expiring30,
    /// 已标记失效，或有效期已过。
    Expired,
    /// 尚未登记资质记录。
    NotRegistered,
}

/// 归一化逗号分隔的供应能力代码。
fn normalize_capability_codes(value: Option<&str>) -> Result<Vec<CapabilityCode>> {
    normalize_code_list(value, "供应能力", |code| match code {
        "physical" => Some(CapabilityCode::Physical),
        "virtual" => Some(CapabilityCode::Virtual),
        "offline_service" => Some(CapabilityCode::OfflineService),
        "api" => Some(CapabilityCode::Api),
        "printing" => Some(CapabilityCode::Printing),
        _ => None,
    })
}

/// 归一化逗号分隔的资质类型代码。
fn normalize_qualification_types(value: Option<&str>) -> Result<Vec<QualificationType>> {
    normalize_code_list(value, "资质类型", |code| match code {
        "certificate" => Some(QualificationType::Certificate),
        "contract" => Some(QualificationType::Contract),
        "authorization" => Some(QualificationType::Authorization),
        "food_license" => Some(QualificationType::FoodLicense),
        "legal_person_id" => Some(QualificationType::LegalPersonId),
        _ => None,
    })
}

/// 清理、去重并校验逗号分隔的固定枚举代码。
fn normalize_code_list<T: Copy + Eq>(
    value: Option<&str>,
    field: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    let mut codes = Vec::new();
    for raw in value.split(',').map(str::trim).filter(|code| !code.is_empty()) {
        let code = parse(raw).ok_or_else(|| Error::ValidationError(format!("不支持的{field}: {raw}")))?;
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    Ok(codes)
}

/// 商务结算版本响应视图（契约形状对齐投影行）。
///
/// `invoice_tax_rate`/`signing_entity_party_id`/`payment_entity_party_id` 不在
/// 仓储投影列中（列表接口返回 `None`），仅详情（实体构造）填充；差异列入
/// P3 报告「契约变更」。
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
            invoice_tax_rate: Some(revision.invoice_tax_rate),
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
    pub valid_from: String,
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
}

impl From<SupplierQualification> for SupplierQualificationView {
    /// 从实体构造响应视图。
    fn from(qualification: SupplierQualification) -> Self {
        Self {
            id: qualification.base.id,
            supplier_id: qualification.supplier_id.to_string(),
            qualification_type: qualification.qualification_type,
            certificate_no: qualification.certificate_no,
            issuer: qualification.issuer,
            valid_from: qualification.valid_from.to_string(),
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

/// 根级供应商资料中的默认联系人输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierProfileContactInput {
    /// 联系人姓名。
    #[validate(custom(function = "non_blank", message = "联系人姓名不能为空"))]
    pub contact_name: String,
    /// 手机号明文；仅在请求处理期间存在。
    #[validate(custom(function = "non_blank", message = "手机号不能为空"))]
    pub mobile: String,
    /// 固话。
    pub telephone: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
}

/// 根级供应商资料中的默认经营地址输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierProfileAddressInput {
    /// 地址明文；仅在请求处理期间存在。
    #[validate(custom(function = "non_blank", message = "地址不能为空"))]
    pub address: String,
    /// 地址联系人。
    pub contact_name: Option<String>,
}

/// 根级供应商资料中的默认银行账户输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierProfileBankAccountInput {
    /// 银行名称。
    #[validate(custom(function = "non_blank", message = "银行名称不能为空"))]
    pub bank_name: String,
    /// 银行账号明文；仅在请求处理期间存在。
    #[validate(custom(function = "non_blank", message = "银行账号不能为空"))]
    pub account_number: String,
}

/// 根级供应商资料中的资质输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierProfileQualificationInput {
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书或合同编号。
    #[validate(custom(function = "non_blank", message = "证书编号不能为空"))]
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效日期。
    pub valid_from: BusinessDate,
    /// 失效日期。
    pub valid_to: Option<BusinessDate>,
    /// 文件资产。
    pub attachment_id: Option<FileAssetId>,
    /// 适用能力代码；服务端解析为当前供应商能力 ID。
    pub capability_codes: Vec<CapabilityCode>,
}

/// 根级供应商资料中的当前评级输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierProfileRatingInput {
    /// 首次合作期初评分。
    pub initial_score: Option<u8>,
    /// 评级。
    pub rating: SupplierRating,
    /// 当前评分。
    pub current_score: u8,
    /// 生效日期。
    pub valid_from: BusinessDate,
}

/// 创建或修订完整供应商资料的根级命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SaveSupplierProfileRequest {
    /// 客户端幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
    /// 创建时必填的主体编号；修订时忽略。
    pub party_no: Option<String>,
    /// 创建时必填的供应商编号；修订时忽略。
    pub supplier_no: Option<String>,
    /// 修订时必填的主体乐观锁版本。
    pub expected_party_version: Option<u64>,
    /// 修订时必填的供应商乐观锁版本。
    pub expected_supplier_version: Option<u64>,
    /// 法定名称。
    #[validate(custom(function = "non_blank", message = "法定名称不能为空"))]
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 默认联系人；`None` 表示创建时不填、修订时保留。
    pub contact: Option<SupplierProfileContactInput>,
    /// 修订时明确停用当前联系人；不能与 `contact` 同时提交。
    #[serde(default)]
    pub clear_contact: bool,
    /// 默认经营地址；`None` 表示创建时不填、修订时保留。
    pub address: Option<SupplierProfileAddressInput>,
    /// 修订时明确停用当前经营地址；不能与 `address` 同时提交。
    #[serde(default)]
    pub clear_address: bool,
    /// 税号；`None` 表示创建时不填、修订时保留。
    pub tax_no: Option<String>,
    /// 修订时明确停用当前税务档案；不能与非空 `tax_no` 同时提交。
    #[serde(default)]
    pub clear_tax_profile: bool,
    /// 默认银行账户；`None` 表示创建时不填、修订时保留。
    pub bank_account: Option<SupplierProfileBankAccountInput>,
    /// 修订时明确停用当前银行账户；不能与 `bank_account` 同时提交。
    #[serde(default)]
    pub clear_bank_account: bool,
    /// 结算方式。
    pub settlement_mode: SettlementMode,
    /// 对账周期。
    pub reconciliation_cycle: ReconciliationCycle,
    /// 付款条件结构化快照（不得再编码经营类目）。
    #[validate(custom(function = "non_blank", message = "付款条件快照不能为空"))]
    pub payment_term_snapshot: String,
    /// 经营类目；空白表示未登记。旧客户端可能把类目编码进付款条件快照，实体构造时会拆出。
    #[serde(default)]
    pub business_category: Option<String>,
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点。
    pub invoice_tax_rate: Rate,
    /// 签约主体。
    pub signing_entity_party_id: entities::ids::PartyId,
    /// 付款主体。
    pub payment_entity_party_id: entities::ids::PartyId,
    /// 当前启用能力代码集合。
    pub capability_codes: Vec<CapabilityCode>,
    /// 当前资质集合。
    pub qualifications: Vec<SupplierProfileQualificationInput>,
    /// 当前评级；`None` 表示不写评级。
    pub rating: Option<SupplierProfileRatingInput>,
    /// 从属事实生效日期。
    pub effective_from: BusinessDate,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
}

impl SaveSupplierProfileRequest {
    /// 校验根级供应商资料请求的完整输入合同。
    ///
    /// # 返回
    /// 根字段、清空/替换意图及全部嵌套输入均合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 根字段格式非法、同一资料同时请求替换与清空，或嵌套输入非法时返回
    /// `ValidationError`；校验顺序固定为根字段、互斥意图、联系人、地址、
    /// 银行账户、资质。
    pub(crate) fn validate_contract(&self) -> Result<()> {
        self.validate()?;
        if self.clear_contact && self.contact.is_some() {
            return Err(Error::ValidationError("联系人不能同时替换和清空".to_string()));
        }
        if self.clear_address && self.address.is_some() {
            return Err(Error::ValidationError("经营地址不能同时替换和清空".to_string()));
        }
        if self.clear_tax_profile
            && self
                .tax_no
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(Error::ValidationError("税务档案不能同时替换和清空".to_string()));
        }
        if self.clear_bank_account && self.bank_account.is_some() {
            return Err(Error::ValidationError("银行账户不能同时替换和清空".to_string()));
        }
        if let Some(contact) = &self.contact {
            contact.validate()?;
        }
        if let Some(address) = &self.address {
            address.validate()?;
        }
        if let Some(bank_account) = &self.bank_account {
            bank_account.validate()?;
        }
        for qualification in &self.qualifications {
            qualification.validate()?;
        }
        Ok(())
    }
}

/// 根级供应商资料命令的稳定结果，也用于幂等查询。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierProfileMutationView {
    /// 供应商 ID。
    pub supplier_id: String,
    /// 供应商编号。
    pub supplier_no: String,
    /// 当前商务版本 ID。
    pub revision_id: String,
    /// 当前商务版本号。
    pub revision_no: u32,
    /// 保存后的供应商乐观锁版本。
    pub supplier_version: u64,
    /// 命令业务生效日期。
    pub effective_from: String,
    /// 命令记录时间（秒级时间戳）。
    pub recorded_at: u64,
    /// 原始变更原因。
    pub change_reason: String,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::common::time::BusinessDate;
    use entities::ids::PartyId;
    use entities::money::Rate;
    use entities::supplier::{
        CapabilityCode, InvoiceType, QualificationType, ReconciliationCycle, SettlementMode,
    };
    use validator::Validate;

    use super::{
        normalize_sort, SaveSupplierProfileRequest, SortDir, SupplierListParams, SupplierProfileAddressInput,
        SupplierProfileBankAccountInput, SupplierProfileContactInput, SupplierProfileQualificationInput,
        SupplierQualificationHealth,
    };
    use crate::errors::Error;

    fn save_supplier_profile_request() -> SaveSupplierProfileRequest {
        SaveSupplierProfileRequest {
            idempotency_key: "supplier-profile-command-1".to_string(),
            party_no: Some("PARTY-001".to_string()),
            supplier_no: Some("SUP-001".to_string()),
            expected_party_version: None,
            expected_supplier_version: None,
            legal_name: "上海示例供应链有限公司".to_string(),
            short_name: Some("示例供应链".to_string()),
            unified_credit_code: Some("91310000TEST000001".to_string()),
            contact: Some(SupplierProfileContactInput {
                contact_name: "张三".to_string(),
                mobile: "13800000000".to_string(),
                telephone: None,
                email: None,
            }),
            clear_contact: false,
            address: Some(SupplierProfileAddressInput {
                address: "上海市浦东新区示例路 1 号".to_string(),
                contact_name: Some("张三".to_string()),
            }),
            clear_address: false,
            tax_no: Some("91310000TEST000001".to_string()),
            clear_tax_profile: false,
            bank_account: Some(SupplierProfileBankAccountInput {
                bank_name: "中国银行".to_string(),
                account_number: "6222000000000000".to_string(),
            }),
            clear_bank_account: false,
            settlement_mode: SettlementMode::PayAfterUse,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: "POSTPAY_NET30".to_string(),
            business_category: Some("办公用品".to_string()),
            invoice_type: InvoiceType::VatSpecial,
            invoice_tax_rate: Rate::from_str("0.13").unwrap(),
            signing_entity_party_id: PartyId::new("party-signing"),
            payment_entity_party_id: PartyId::new("party-payment"),
            capability_codes: vec![CapabilityCode::Physical],
            qualifications: vec![SupplierProfileQualificationInput {
                qualification_type: QualificationType::Contract,
                certificate_no: "CONTRACT-001".to_string(),
                issuer: None,
                valid_from: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
                valid_to: None,
                attachment_id: None,
                capability_codes: vec![CapabilityCode::Physical],
            }],
            rating: None,
            effective_from: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
            change_reason: "首次登记".to_string(),
        }
    }

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        let (field, direction) = normalize_sort(
            &Some(" supplier_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "supplier_no"],
        )
        .unwrap();
        assert_eq!(field, "supplier_no");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn supplier_profile_bank_account_requires_bank_and_account_number() {
        let valid = SupplierProfileBankAccountInput {
            bank_name: "中国银行".to_string(),
            account_number: "6222000000000000".to_string(),
        };
        assert!(valid.validate().is_ok());

        let missing_bank = SupplierProfileBankAccountInput {
            bank_name: " ".to_string(),
            account_number: "6222000000000000".to_string(),
        };
        assert!(missing_bank.validate().is_err());

        let missing_account_number = SupplierProfileBankAccountInput {
            bank_name: "中国银行".to_string(),
            account_number: " ".to_string(),
        };
        assert!(missing_account_number.validate().is_err());
    }

    #[test]
    fn supplier_profile_contract_accepts_complete_input_and_blank_cleared_tax_number() {
        let request = save_supplier_profile_request();
        assert!(request.validate_contract().is_ok());

        let cleared_tax = SaveSupplierProfileRequest {
            tax_no: Some("   ".to_string()),
            clear_tax_profile: true,
            ..request
        };
        assert!(cleared_tax.validate_contract().is_ok());
    }

    #[test]
    fn supplier_profile_contract_runs_root_validator_before_intent_conflicts() {
        let request = SaveSupplierProfileRequest {
            idempotency_key: " ".to_string(),
            clear_contact: true,
            clear_address: true,
            ..save_supplier_profile_request()
        };

        let error = request.validate_contract().unwrap_err();
        match error {
            Error::ValidationError(message) => {
                assert!(message.contains("幂等键不能为空"));
                assert!(!message.contains("联系人不能同时替换和清空"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn supplier_profile_contract_preserves_clear_intent_error_order_and_text() {
        let request = SaveSupplierProfileRequest {
            clear_contact: true,
            clear_address: true,
            clear_bank_account: true,
            ..save_supplier_profile_request()
        };

        assert!(matches!(
            request.validate_contract(),
            Err(Error::ValidationError(message)) if message == "联系人不能同时替换和清空"
        ));
    }

    #[test]
    fn supplier_profile_contract_validates_nested_inputs_in_stable_order() {
        let mut request = save_supplier_profile_request();
        request.contact.as_mut().unwrap().contact_name = " ".to_string();
        request.address.as_mut().unwrap().address = " ".to_string();

        let error = request.validate_contract().unwrap_err();
        match error {
            Error::ValidationError(message) => {
                assert!(message.contains("联系人姓名不能为空"));
                assert!(!message.contains("地址不能为空"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn supplier_list_normalizes_repeated_filter_codes() {
        let params = SupplierListParams {
            keyword: None,
            party_id: None,
            status: None,
            capability_codes: Some(" physical, api,physical ".to_string()),
            qualification_types: Some("contract, food_license,contract".to_string()),
            qualification_health: Some(SupplierQualificationHealth::Expiring30),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };

        let query = params.normalized().unwrap();
        assert_eq!(query.capability_codes.len(), 2);
        assert_eq!(query.qualification_types.len(), 2);
        assert_eq!(
            query.qualification_health,
            Some(SupplierQualificationHealth::Expiring30)
        );
    }

    #[test]
    fn supplier_list_rejects_unknown_filter_code() {
        let params = SupplierListParams {
            keyword: None,
            party_id: None,
            status: None,
            capability_codes: Some("unknown".to_string()),
            qualification_types: None,
            qualification_health: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };

        assert!(params.normalized().is_err());
    }
}
