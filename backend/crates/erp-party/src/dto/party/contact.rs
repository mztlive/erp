//! 联系人创建、更新、视图与列表查询 DTO。

use application_core::non_blank;
use erp_core::common::time::BusinessDate;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::masked_last4;
use crate::entity::party::{EffectiveRecordStatus, PartyContact};

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
    /// 手机号掩码；列表与详情均不返回明文。
    pub mobile_masked: String,
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
            mobile_masked: masked_last4(&contact.mobile_last4),
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
