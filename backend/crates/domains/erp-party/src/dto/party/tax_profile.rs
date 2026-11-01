//! 税务资料创建、更新、视图与列表查询 DTO。

use application_core::non_blank;
use erp_core::common::time::BusinessDate;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::party::{EffectiveRecordStatus, PartyTaxProfile};

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

impl From<PartyTaxProfile> for PartyTaxProfileView {
    /// 从实体构造响应视图。
    fn from(profile: PartyTaxProfile) -> Self {
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
