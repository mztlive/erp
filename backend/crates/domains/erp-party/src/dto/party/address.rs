//! 地址创建、更新、视图与列表查询 DTO。

use application_core::non_blank;
use erp_core::common::time::BusinessDate;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::party::{AddressType, EffectiveRecordStatus, PartyAddress};

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

impl From<PartyAddress> for PartyAddressView {
    /// 从实体构造响应视图。
    fn from(address: PartyAddress) -> Self {
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
