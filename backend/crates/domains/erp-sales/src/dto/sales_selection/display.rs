//! 管理端选品册陈列 DTO。公开页使用独立字段白名单。

use erp_core::money::Amount;
use serde::{Deserialize, Serialize};

use crate::entity::sales_selection::{SelectionPriceComparison, SpecificationAttributeSnapshot};

/// 陈列项视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisplayItemView {
    /// 仅管理端展示的原价和累计加价比例。
    #[serde(flatten)]
    pub price_comparison: SelectionPriceComparison,
    /// 单品的册内 SKU 身份，仅管理端改价使用；套餐为空。
    #[serde(default)]
    pub sku_id: Option<String>,
    /// 项身份。
    pub id: String,
    /// 与 `id` 相同，兼容预览卡片。
    pub item_id: String,
    /// `SINGLE_SKU` 或 `PACKAGE`。
    pub kind: String,
    /// 是否已删除。
    pub removed: bool,
    /// 档位。
    pub tier_id: Option<String>,
    /// 档位名称。
    pub tier_name: Option<String>,
    /// 名称。
    pub name: String,
    /// 规格。
    pub specification: Vec<SpecificationAttributeSnapshot>,
    /// 规格展示文案。
    pub spec_label: String,
    /// 售价。
    pub price: Amount,
    /// 与 `price` 相同。
    pub price_gross: Amount,
    /// 与目标差额。
    pub target_delta: Option<Amount>,
    /// 封面资产。
    pub cover_asset_id: Option<String>,
    /// 管理端封面引用，由前端转预览。
    pub cover_image: Option<String>,
    /// 单位。
    pub unit: Option<String>,
    /// 成员。
    pub members: Vec<DisplayMemberView>,
    /// 是否缺图。
    pub missing_image: bool,
}

/// 套餐成员。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisplayMemberView {
    /// 成员商品原价与累计加价比例。
    #[serde(flatten)]
    pub price_comparison: SelectionPriceComparison,
    /// SKU 稳定身份；公开页不展示编号，内部预览可用。
    pub sku_id: String,
    /// 名称。
    pub name: String,
    /// 规格。
    pub specification: Vec<SpecificationAttributeSnapshot>,
    /// 单位。
    pub unit: String,
    /// 售价。
    pub price: Amount,
}
