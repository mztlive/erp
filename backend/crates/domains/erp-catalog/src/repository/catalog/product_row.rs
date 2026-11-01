//! 公司商品列表投影行，来源差异摘要与原有商品字段共用分页。
use serde::{Deserialize, Serialize};

use crate::entity::catalog::{EnableStatus, ProductKind, ProductListingStatus};
use crate::source_sync::SourceField;

/// 商品列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductRow {
    /// 实体主键。
    pub id: String,
    /// 商品编号。
    pub product_no: String,
    /// 商品业务类型。
    pub product_kind: ProductKind,
    /// 当前商品名称。
    pub name: Option<String>,
    /// 当前商品分类 ID。
    pub category_id: Option<String>,
    /// 当前商品品牌 ID。
    pub brand_id: Option<String>,
    /// 当前维护人。
    #[serde(default)]
    pub maintainer_user_id: String,
    /// 当前业务组织。
    #[serde(default)]
    pub business_org_unit_id: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 从当前启用 SKU 继承的上架状态。
    pub listing_status: ProductListingStatus,
    /// 当前已上架 SKU 数。
    pub listed_sku_count: u32,
    /// 当前启用 SKU 总数。
    pub sku_count: u32,
    /// 当前存在有效供给关系的启用 SKU 数。
    pub supplied_sku_count: u32,
    /// 当前已填写销售价的启用 SKU 数。
    pub priced_sku_count: u32,
    /// 供应商资料待确认字段数。
    #[serde(default)]
    pub source_diff_count: u32,
    /// 供应商资料待确认字段。
    #[serde(default)]
    pub source_diff_fields: Vec<SourceField>,
    /// 当前商品修订 ID。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}
