//! 域 D07 `party` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；业务日期一律 `YYYY-MM-DD`；时间一律秒级时间戳。
//! 敏感值（手机号、地址、银行账号）只出现在创建入参，响应视图不返回明文
//! （数据模型 §4.5.5：实体只保存带密钥 HMAC 指纹，明文永不入库）。

use entities::common::time::BusinessDate;
use entities::party::{
    AddressType, EffectiveRecordStatus, Party, PartyContact, PartyKind, PartyRevision, PartyStatus,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 主体列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const PARTY_SORT_FIELDS: &[&str] = &["created_at", "party_no", "status"];
/// 主体修订列表允许的排序字段白名单。
pub(crate) const PARTY_REVISION_SORT_FIELDS: &[&str] =
    &["created_at", "revision_no", "effective_from", "effective_to"];
/// 联系人列表允许的排序字段白名单。
pub(crate) const PARTY_CONTACT_SORT_FIELDS: &[&str] = &["created_at", "contact_name", "valid_from"];
/// 地址列表允许的排序字段白名单。
pub(crate) const PARTY_ADDRESS_SORT_FIELDS: &[&str] = &["created_at", "address_type", "valid_from"];
/// 税务资料列表允许的排序字段白名单。
pub(crate) const PARTY_TAX_PROFILE_SORT_FIELDS: &[&str] = &["created_at", "tax_no", "valid_from"];
/// 银行账户列表允许的排序字段白名单。
pub(crate) const PARTY_BANK_ACCOUNT_SORT_FIELDS: &[&str] = &["created_at", "bank_account_no", "valid_from"];

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

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串不生效）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 主体创建请求（HTTP 契约：`{ party_no, legal_name, ... }`）。
///
/// 同事务建立 `party` + 首版 `party_revision`（§6.2：稳定主体 + 不可变修订）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePartyRequest {
    /// 主体编号（全局唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "主体编号不能为空"))]
    pub party_no: String,
    /// 主体类型；缺省视为企业组织（当前只使用企业组织）。
    pub party_kind: Option<PartyKind>,
    /// 统一社会信用代码（18 位字母数字；允许历史数据为空）。
    pub unified_credit_code: Option<String>,
    /// 法定名称（首版修订快照）。
    #[validate(custom(function = "non_blank", message = "法定名称不能为空"))]
    pub legal_name: String,
    /// 简称（首版修订快照）。
    pub short_name: Option<String>,
    /// 生效开始日期。
    pub effective_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 启停状态；缺省视为启用。
    pub status: Option<PartyStatus>,
}

/// 主体更新请求（乐观锁 + 形成新修订）。
///
/// 每次更新必带一份新的 `party_revision` 快照（W03：保存即形成新版本）；
/// `status`/`unified_credit_code` 为可选表头字段变更。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePartyRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 启停状态；缺省表示不修改。
    pub status: Option<PartyStatus>,
    /// 统一社会信用代码；`None` 表示不修改，空字符串表示清除。
    pub unified_credit_code: Option<String>,
    /// 法定名称（新修订快照）。
    #[validate(custom(function = "non_blank", message = "法定名称不能为空"))]
    pub legal_name: String,
    /// 简称（新修订快照）；`None` 表示不修改。
    pub short_name: Option<String>,
    /// 生效开始日期。
    pub effective_from: BusinessDate,
    /// 生效结束日期。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
}

/// 主体响应视图（列表用，契约形状对齐 `party` 投影行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyView {
    /// 实体主键。
    pub id: String,
    /// 主体编号。
    pub party_no: String,
    /// 主体类型。
    pub party_kind: PartyKind,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 启停状态。
    pub status: PartyStatus,
    /// 当前生效修订 ID。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<Party> for PartyView {
    /// 从实体构造响应视图。
    fn from(party: Party) -> Self {
        Self {
            id: party.base.id,
            party_no: party.party_no,
            party_kind: party.party_kind,
            unified_credit_code: party.unified_credit_code,
            status: party.stable.status,
            current_revision_id: party.stable.current_revision_id,
            version: party.base.version,
            created_at: party.base.created_at,
        }
    }
}

/// 主体修订响应视图（契约形状对齐 `party_revision` 投影行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyRevisionView {
    /// 实体主键。
    pub id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 法定名称。
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
    /// 生效开始日期。
    pub effective_from: String,
    /// 生效结束日期。
    pub effective_to: Option<String>,
    /// 变更原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<PartyRevision> for PartyRevisionView {
    /// 从实体构造响应视图。
    fn from(revision: PartyRevision) -> Self {
        Self {
            id: revision.base.id,
            revision_no: revision.revision.revision_no,
            legal_name: revision.legal_name,
            short_name: revision.short_name,
            effective_from: revision.effective_from.to_string(),
            effective_to: revision.effective_to.map(|date| date.to_string()),
            change_reason: revision.change_reason,
            version: revision.base.version,
            created_at: revision.base.created_at,
        }
    }
}

/// 主体列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PartyListParams {
    /// 主体编号模糊搜索。
    pub keyword: Option<String>,
    /// 主体类型筛选。
    pub party_kind: Option<PartyKind>,
    /// 启停状态筛选。
    pub status: Option<PartyStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`party_no`/`status`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的主体列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartyListQuery {
    /// 主体编号模糊搜索。
    pub keyword: Option<String>,
    /// 主体类型筛选。
    pub party_kind: Option<PartyKind>,
    /// 启停状态筛选。
    pub status: Option<PartyStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl PartyListParams {
    /// 归一化主体列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<PartyListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, PARTY_SORT_FIELDS)?;
        Ok(PartyListQuery {
            keyword: normalized_text(self.keyword.as_deref()),
            party_kind: self.party_kind,
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

/// 主体修订列表查询参数（`party_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PartyRevisionListParams {
    /// 法定名称模糊匹配。
    pub legal_name: Option<String>,
    /// 简称模糊匹配。
    pub short_name: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`revision_no`/`effective_from`/`effective_to`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 联系人创建请求（HTTP 契约：手机号为明文入参，实体只保留指纹与密文）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePartyContactRequest {
    /// 联系人姓名。
    #[validate(custom(function = "non_blank", message = "联系人姓名不能为空"))]
    pub contact_name: String,
    /// 职务/用途。
    pub title: Option<String>,
    /// 手机号（明文入参；低熵敏感值 §4.5.5）。
    #[validate(custom(function = "non_blank", message = "手机号不能为空"))]
    pub mobile: String,
    /// 电话。
    pub telephone: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认联系人。
    pub is_default: bool,
    /// 启停状态；缺省视为启用。
    pub status: Option<EffectiveRecordStatus>,
}

/// 联系人更新请求（仅生命周期字段：启停、结束有效期、默认标记）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePartyContactRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EffectiveRecordStatus>,
    /// 生效结束日期（`Set` 时校验晚于 `valid_from`）；`None` 表示不修改。
    pub valid_to: Option<BusinessDate>,
    /// 默认标记；`None` 表示不修改。
    pub is_default: Option<bool>,
}

/// 联系人响应视图（契约形状对齐 `party_contact` 投影行；不含敏感字段）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyContactView {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 联系人姓名。
    pub contact_name: String,
    /// 职务/用途。
    pub title: Option<String>,
    /// 电话。
    pub telephone: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认联系人。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<PartyContact> for PartyContactView {
    /// 从实体构造响应视图。
    fn from(contact: PartyContact) -> Self {
        Self {
            id: contact.base.id,
            party_id: contact.party_id.to_string(),
            contact_name: contact.contact_name,
            title: contact.title,
            telephone: contact.telephone,
            email: contact.email,
            valid_from: contact.valid_from.to_string(),
            valid_to: contact.valid_to.map(|date| date.to_string()),
            is_default: contact.is_default,
            status: contact.status,
            version: contact.base.version,
            created_at: contact.base.created_at,
        }
    }
}

/// 联系人列表查询参数（`party_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PartyContactListParams {
    /// 联系人姓名模糊搜索。
    pub keyword: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`contact_name`/`valid_from`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 地址创建请求（HTTP 契约为结构化地址明文；实体只保留指纹与密文）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePartyAddressRequest {
    /// 地址类型。
    pub address_type: AddressType,
    /// 联系人。
    pub contact_name: Option<String>,
    /// 地址内容（明文入参；履约地址为低熵敏感值 §4.5.5）。
    #[validate(custom(function = "non_blank", message = "地址不能为空"))]
    pub address: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认地址。
    pub is_default: bool,
    /// 启停状态；缺省视为启用。
    pub status: Option<EffectiveRecordStatus>,
}

/// 地址更新请求（仅生命周期字段）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePartyAddressRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EffectiveRecordStatus>,
    /// 生效结束日期；`None` 表示不修改。
    pub valid_to: Option<BusinessDate>,
    /// 默认标记；`None` 表示不修改。
    pub is_default: Option<bool>,
}

/// 地址响应视图（契约形状对齐 `party_address` 投影行；不含敏感字段）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyAddressView {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 地址类型。
    pub address_type: AddressType,
    /// 联系人。
    pub contact_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认地址。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<entities::party::PartyAddress> for PartyAddressView {
    /// 从实体构造响应视图。
    fn from(address: entities::party::PartyAddress) -> Self {
        Self {
            id: address.base.id,
            party_id: address.party_id.to_string(),
            address_type: address.address_type,
            contact_name: address.contact_name,
            valid_from: address.valid_from.to_string(),
            valid_to: address.valid_to.map(|date| date.to_string()),
            is_default: address.is_default,
            status: address.status,
            version: address.base.version,
            created_at: address.base.created_at,
        }
    }
}

/// 地址列表查询参数（`party_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PartyAddressListParams {
    /// 地址类型筛选。
    pub address_type: Option<AddressType>,
    /// 启停状态筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`address_type`/`valid_from`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 税务资料创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePartyTaxProfileRequest {
    /// 纳税人识别号（统一社会信用代码或旧税号）。
    #[validate(custom(function = "non_blank", message = "税号不能为空"))]
    pub tax_no: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认税务资料。
    pub is_default: bool,
    /// 启停状态；缺省视为启用。
    pub status: Option<EffectiveRecordStatus>,
}

/// 税务资料更新请求（仅生命周期字段）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePartyTaxProfileRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EffectiveRecordStatus>,
    /// 生效结束日期；`None` 表示不修改。
    pub valid_to: Option<BusinessDate>,
    /// 默认标记；`None` 表示不修改。
    pub is_default: Option<bool>,
}

/// 税务资料响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyTaxProfileView {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 纳税人识别号。
    pub tax_no: String,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认税务资料。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<entities::party::PartyTaxProfile> for PartyTaxProfileView {
    /// 从实体构造响应视图。
    fn from(profile: entities::party::PartyTaxProfile) -> Self {
        Self {
            id: profile.base.id,
            party_id: profile.party_id.to_string(),
            tax_no: profile.tax_no,
            valid_from: profile.valid_from.to_string(),
            valid_to: profile.valid_to.map(|date| date.to_string()),
            is_default: profile.is_default,
            status: profile.status,
            version: profile.base.version,
            created_at: profile.base.created_at,
        }
    }
}

/// 税务资料列表查询参数（`party_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PartyTaxProfileListParams {
    /// 启停状态筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`tax_no`/`valid_from`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 银行账户创建请求（HTTP 契约为账号明文；实体只保留指纹与密文）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePartyBankAccountRequest {
    /// ERP 内部稳定账户编号（全局唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "账户编号不能为空"))]
    pub bank_account_no: String,
    /// 户名。
    #[validate(custom(function = "non_blank", message = "户名不能为空"))]
    pub account_name: String,
    /// 银行名称。
    #[validate(custom(function = "non_blank", message = "银行名称不能为空"))]
    pub bank_name: String,
    /// 支行名称。
    pub bank_branch_name: Option<String>,
    /// 账号（明文入参；低熵敏感值 §4.5.5）。
    #[validate(custom(function = "non_blank", message = "账号不能为空"))]
    pub account_number: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认账户。
    pub is_default: bool,
    /// 启停状态；缺省视为启用。
    pub status: Option<EffectiveRecordStatus>,
}

/// 银行账户更新请求（仅生命周期字段）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePartyBankAccountRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EffectiveRecordStatus>,
    /// 生效结束日期；`None` 表示不修改。
    pub valid_to: Option<BusinessDate>,
    /// 默认标记；`None` 表示不修改。
    pub is_default: Option<bool>,
}

/// 银行账户响应视图（契约形状对齐 `party_bank_account` 投影行；不含敏感字段）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyBankAccountView {
    /// 实体主键。
    pub id: String,
    /// ERP 内部稳定账户编号。
    pub bank_account_no: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 户名。
    pub account_name: String,
    /// 银行名称。
    pub bank_name: String,
    /// 支行名称。
    pub bank_branch_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认账户。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<entities::party::PartyBankAccount> for PartyBankAccountView {
    /// 从实体构造响应视图。
    fn from(account: entities::party::PartyBankAccount) -> Self {
        Self {
            id: account.base.id,
            bank_account_no: account.bank_account_no,
            party_id: account.party_id.to_string(),
            account_name: account.account_name,
            bank_name: account.bank_name,
            bank_branch_name: account.bank_branch_name,
            valid_from: account.valid_from.to_string(),
            valid_to: account.valid_to.map(|date| date.to_string()),
            is_default: account.is_default,
            status: account.status,
            version: account.base.version,
            created_at: account.base.created_at,
        }
    }
}

/// 银行账户列表查询参数（`party_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PartyBankAccountListParams {
    /// 启停状态筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`bank_account_no`/`valid_from`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, PartyListParams, SortDir};
    use entities::party::PartyStatus;
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" party_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "party_no"],
        )
        .unwrap();
        assert_eq!(field, "party_no");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn party_list_params_normalize_paging_filters_and_sort_defaults() {
        let params = PartyListParams {
            keyword: Some(" P-20 ".to_string()),
            party_kind: None,
            status: Some(PartyStatus::Active),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.keyword.as_deref(), Some("P-20"));
        assert_eq!(query.status, Some(PartyStatus::Active));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = PartyListParams {
            keyword: None,
            party_kind: None,
            status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn create_party_request_deserializes_contract_shape() {
        let request: super::CreatePartyRequest = serde_json::from_value(json!({
            "party_no": "P-2026-001",
            "legal_name": "上海示例科技有限公司",
            "effective_from": "2026-01-01",
            "change_reason": "首次建档",
        }))
        .unwrap();
        assert_eq!(request.party_no, "P-2026-001");
        assert!(request.status.is_none(), "status 缺省由 Service 按启用处理");
    }
}
