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
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 供应商角色列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const SUPPLIER_SORT_FIELDS: &[&str] = &["created_at", "supplier_no", "status"];
/// 商务结算版本列表允许的排序字段白名单。
pub(crate) const COMMERCIAL_PROFILE_SORT_FIELDS: &[&str] =
    &["created_at", "revision_no", "valid_from", "valid_to"];
/// 能力列表允许的排序字段白名单。
pub(crate) const CAPABILITY_SORT_FIELDS: &[&str] = &["created_at", "capability_code", "valid_to"];
/// 资质列表允许的排序字段白名单。
pub(crate) const QUALIFICATION_SORT_FIELDS: &[&str] = &["created_at", "valid_to", "status"];
/// 评估版本列表允许的排序字段白名单。
pub(crate) const RATING_SORT_FIELDS: &[&str] = &["created_at", "revision_no", "valid_from", "valid_to"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

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
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 供应商角色创建请求。
///
/// 同事务建立 `supplier_account` + 首个 `supplier_commercial_profile_revision`
/// （§6.2：供应商角色携带 `current_commercial_profile_revision_id` 指向当前
/// 版本）；`party` 必须已存在（D07 跨域读校验）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierRequest {
    /// 共用企业主体 ID。
    pub party_id: entities::ids::PartyId,
    /// 供应商编号（全局唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "供应商编号不能为空"))]
    pub supplier_no: String,
    /// 默认供应商结算条件引用（受控码表字典）。
    pub default_payment_term_id: Option<String>,
    /// 结算方式（首版商务版本快照）。
    pub settlement_mode: SettlementMode,
    /// 对账周期（首版商务版本快照）。
    pub reconciliation_cycle: ReconciliationCycle,
    /// 结构化付款条件快照（首版商务版本快照）。
    #[validate(custom(function = "non_blank", message = "付款条件快照不能为空"))]
    pub payment_term_snapshot: String,
    /// 发票类型（首版商务版本快照）。
    pub invoice_type: InvoiceType,
    /// 发票税点（如 `0.13` 表示 13%；定点小数，[0, 1)）。
    pub invoice_tax_rate: Rate,
    /// 与我司签约的公司主体（D07 跨域校验存在）。
    pub signing_entity_party_id: entities::ids::PartyId,
    /// 付款时的公司主体（D07 跨域校验存在）。
    pub payment_entity_party_id: entities::ids::PartyId,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 启停状态；缺省视为启用。
    pub status: Option<SupplierAccountStatus>,
}

/// 供应商角色更新请求（乐观锁；`party_id` 与 `supplier_no` 为稳定身份不可修改）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSupplierRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 默认结算条件；`None` 表示不修改，空字符串表示清除。
    pub default_payment_term_id: Option<String>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<SupplierAccountStatus>,
}

/// 供应商角色响应视图（列表用，契约形状对齐 `supplier_account` 投影行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierView {
    /// 实体主键。
    pub id: String,
    /// 共用企业主体 ID。
    pub party_id: String,
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
}

impl From<SupplierAccount> for SupplierView {
    /// 从实体构造响应视图。
    fn from(account: SupplierAccount) -> Self {
        Self {
            id: account.base.id,
            party_id: account.party_id.to_string(),
            supplier_no: account.supplier_no,
            default_payment_term_id: account.default_payment_term_id,
            current_commercial_profile_revision_id: account
                .current_commercial_profile_revision_id
                .map(|id| id.to_string()),
            status: account.stable.status,
            version: account.base.version,
            created_at: account.base.created_at,
        }
    }
}

/// 供应商角色详情视图：供应商 + 当前商务结算版本 + 主体编号。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierDetailView {
    /// 供应商角色响应视图。
    #[serde(flatten)]
    pub account: SupplierView,
    /// 企业主体编号（D07 跨域读）。
    pub party_no: Option<String>,
    /// 当前商务结算版本。
    pub current_profile: Option<CommercialProfileView>,
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
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 商务结算版本创建请求（追加新版本并推进 `current_commercial_profile_revision_id`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCommercialProfileRequest {
    /// 结算方式。
    pub settlement_mode: SettlementMode,
    /// 对账周期。
    pub reconciliation_cycle: ReconciliationCycle,
    /// 结构化付款条件快照。
    #[validate(custom(function = "non_blank", message = "付款条件快照不能为空"))]
    pub payment_term_snapshot: String,
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点（`[0, 1)` 定点小数）。
    pub invoice_tax_rate: Rate,
    /// 与我司签约的公司主体（D07 跨域校验存在）。
    pub signing_entity_party_id: entities::ids::PartyId,
    /// 付款时的公司主体（D07 跨域校验存在）。
    pub payment_entity_party_id: entities::ids::PartyId,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
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
    /// 付款条件快照。
    pub payment_term_snapshot: String,
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点（详情返回，列表为 `None`）。
    pub invoice_tax_rate: Option<Rate>,
    /// 签约主体（详情返回，列表为 `None`）。
    pub signing_entity_party_id: Option<String>,
    /// 付款主体（详情返回，列表为 `None`）。
    pub payment_entity_party_id: Option<String>,
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

impl From<SupplierCommercialProfileRevision> for CommercialProfileView {
    /// 从实体构造响应视图。
    fn from(revision: SupplierCommercialProfileRevision) -> Self {
        Self {
            id: revision.base.id,
            supplier_id: revision.supplier_id.to_string(),
            revision_no: revision.revision.revision_no,
            settlement_mode: revision.settlement_mode,
            reconciliation_cycle: revision.reconciliation_cycle,
            payment_term_snapshot: revision.payment_term_snapshot,
            invoice_type: revision.invoice_type,
            invoice_tax_rate: Some(revision.invoice_tax_rate),
            signing_entity_party_id: Some(revision.signing_entity_party_id.to_string()),
            payment_entity_party_id: Some(revision.payment_entity_party_id.to_string()),
            valid_from: revision.valid_from.to_string(),
            valid_to: revision.valid_to.map(|date| date.to_string()),
            change_reason: revision.change_reason,
            version: revision.base.version,
            created_at: revision.base.created_at,
        }
    }
}

/// 商务结算版本列表查询参数（`supplier_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CommercialProfileListParams {
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`revision_no`/`valid_from`/`valid_to`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 供应商能力创建请求（同事务建立能力 + 首版能力修订）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierCapabilityRequest {
    /// 能力代码（同一供应商内唯一，创建后不可修改）。
    pub capability_code: CapabilityCode,
    /// 服务区域结构化引用。
    pub service_region: Option<String>,
    /// 负责人。
    #[validate(custom(function = "non_blank", message = "负责人不能为空"))]
    pub owner_user_id: String,
    /// 履约说明。
    pub fulfillment_note: Option<String>,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 启停状态；缺省视为启用。
    pub status: Option<CapabilityStatus>,
}

/// 供应商能力更新请求（乐观锁；内容变更追加能力修订快照）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSupplierCapabilityRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 服务区域；`None` 表示不修改，空字符串表示清除。
    pub service_region: Option<String>,
    /// 负责人；`None` 表示不修改。
    pub owner_user_id: Option<String>,
    /// 履约说明；`None` 表示不修改，空字符串表示清除。
    pub fulfillment_note: Option<String>,
    /// 生效结束日期；`None` 表示不修改。
    pub valid_to: Option<BusinessDate>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<CapabilityStatus>,
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
            version: capability.base.version,
            created_at: capability.base.created_at,
        }
    }
}

/// 供应商能力列表查询参数（`supplier_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierCapabilityListParams {
    /// 能力代码筛选。
    pub capability_code: Option<CapabilityCode>,
    /// 启停状态筛选。
    pub status: Option<CapabilityStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`capability_code`/`valid_to`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 供应商资质创建请求（同事务建立资质 + 首版修订 + 适用能力关联行）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierQualificationRequest {
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书编号（供应商、类型、编号组合唯一；合同/授权书用对应编号）。
    #[validate(custom(function = "non_blank", message = "证书编号不能为空"))]
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效、失效日期。
    pub valid_from: BusinessDate,
    /// 失效日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 资质附件（D05 file_asset 跨域校验存在）。
    pub attachment_id: Option<FileAssetId>,
    /// 适用能力 ID 集合（必须属于该供应商，§6.2 资质 ↔ 能力关联）。
    pub capability_ids: Vec<SupplierCapabilityId>,
    /// 状态；缺省视为有效。
    pub status: Option<QualificationStatus>,
}

/// 供应商资质更新请求（乐观锁；内容变更追加资质修订快照）。
///
/// 适用能力关联行不在本接口内整体替换（仓储层暂无 replace 入口，走地基修订）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSupplierQualificationRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 发证机构；`None` 表示不修改，空字符串表示清除。
    pub issuer: Option<String>,
    /// 资质附件；`None` 表示不修改，空字符串表示清除。
    pub attachment_id: Option<String>,
    /// 失效日期；`None` 表示不修改。
    pub valid_to: Option<BusinessDate>,
    /// 状态；`None` 表示不修改。
    pub status: Option<QualificationStatus>,
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
            version: qualification.base.version,
            created_at: qualification.base.created_at,
        }
    }
}

/// 供应商资质列表查询参数（`supplier_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierQualificationListParams {
    /// 资质类型筛选。
    pub qualification_type: Option<QualificationType>,
    /// 资质状态筛选。
    pub status: Option<QualificationStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`valid_to`/`status`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 供应商评估版本创建请求（追加版本；期初评分只在首次版本允许填写）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierRatingRequest {
    /// 合作期初评分（百分制；只在首次合作版本填写）。
    pub initial_score: Option<u8>,
    /// 供应商评级。
    pub rating: SupplierRating,
    /// 合作中评分（百分制）。
    pub current_score: u8,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
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

/// 供应商评估版本列表查询参数（`supplier_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierRatingListParams {
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`revision_no`/`valid_from`/`valid_to`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 从「附件更新入参」映射清除意图：空字符串表示清除，其余原样。
///
/// # 参数
/// * `value` - 请求携带的附件 ID 或空字符串
///
/// # 返回
/// 返回归一化后的可选附件 ID。
pub(crate) fn attachment_update(value: Option<String>) -> Option<Option<FileAssetId>> {
    match value {
        Some(raw) if raw.trim().is_empty() => Some(None),
        Some(raw) => Some(Some(FileAssetId::new(raw))),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, SortDir};

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
    fn attachment_update_maps_clear_and_set() {
        assert_eq!(super::attachment_update(None), None);
        assert_eq!(super::attachment_update(Some("   ".to_string())), Some(None));
        assert_eq!(
            super::attachment_update(Some("file-1".to_string())),
            Some(Some(entities::ids::FileAssetId::new("file-1")))
        );
    }
}
