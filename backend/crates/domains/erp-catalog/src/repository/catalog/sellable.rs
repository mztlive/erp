//! 可售查询消费的筛选条件与稳定行合同。
use erp_core::common::time::BusinessDate;
use erp_core::delivery_coverage::DeliveryCoverageQuery;
use erp_core::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};

use crate::entity::catalog::{ProductKind, SellablePriceKind};

/// 公司商品池列表筛选条件。
///
/// 资格硬条件（启用、一件代发价、有效供给等）由聚合管道固定施加；本结构只承载
/// 调用方可选的业务筛选。供应商身份仅在内部传递，用于筛选与批量补齐业务编号。
#[derive(Debug, Clone)]
pub struct SellableSkuFilter {
    /// SKU 编号、SKU 名称、商品编号、商品名称、规格或条码关键字；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 商品业务类型；`None` 表示不筛选。
    pub product_kind: Option<ProductKind>,
    /// 当前商品分类；`None` 表示不筛选。
    pub category_id: Option<String>,
    /// 当前商品品牌；`None` 表示不筛选。
    pub brand_id: Option<String>,
    /// 当前有效供给中的供应商；`None` 表示不筛选。
    pub supplier_id: Option<String>,
    /// 当前有效供给去重供应商数量上限（含）；`None` 表示不按供应保障筛选。
    pub max_supplier_count: Option<u32>,
    /// 价格筛选口径；单个区间只匹配一种价格。
    pub price_kind: SellablePriceKind,
    /// 所选含税参考价下限（含）；`None` 表示无下限。
    pub sales_price_min: Option<Amount>,
    /// 所选含税参考价上限（含）；`None` 表示无上限。
    pub sales_price_max: Option<Amount>,
    /// 按配送地区筛选；`None` 表示不筛选。按预算覆盖匹配，允许短暂滞后。
    pub delivery: Option<DeliveryCoverageQuery>,
    /// 只在这些 SKU 中查询（勾选导出与复核）；`None` 表示不限。
    pub sku_ids: Option<Vec<String>>,
    /// 排序；默认按 SKU 编号升序。
    pub sort: SellableSort,
    /// 服务端解释的销售资格业务日期。
    pub eligibility_as_of: BusinessDate,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
}

impl SellableSkuFilter {
    /// 构造仅携带资格业务日、无业务筛选的空条件（用于精确修订复核）。
    ///
    /// # 参数
    /// * `eligibility_as_of` - 资格业务日期
    ///
    /// # 返回
    /// 返回无可选筛选、分页占位为 1 的过滤条件。
    ///
    /// # 错误
    /// 无。
    pub fn as_of(eligibility_as_of: BusinessDate) -> Self {
        Self {
            keyword: None,
            product_kind: None,
            category_id: None,
            brand_id: None,
            supplier_id: None,
            max_supplier_count: None,
            price_kind: SellablePriceKind::default(),
            sales_price_min: None,
            sales_price_max: None,
            delivery: None,
            sku_ids: None,
            sort: SellableSort::default(),
            eligibility_as_of,
            page: 1,
            page_size: 1,
        }
    }
}

/// 公司商品池只读查询行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SellableSkuRow {
    /// 稳定 SKU ID。
    pub sku_id: String,
    /// 稳定 SKU 乐观锁版本。
    pub sku_version: u64,
    /// 当前且符合资格的 SKU 修订 ID。
    pub sku_revision_id: String,
    /// 当前 SKU 修订号。
    pub sku_revision_no: u32,
    /// SKU 编码。
    pub sku_no: String,
    /// 所属稳定商品 ID。
    pub product_id: String,
    /// 商品编码。
    pub product_no: String,
    /// 商品业务类型。
    pub product_kind: ProductKind,
    /// 公司审核后的 SKU 名称。
    pub name: String,
    /// 稳定 SKU 的规范化规格属性签名。
    pub specification_signature: String,
    /// 公司审核后的规格文案。
    pub specification: Option<String>,
    /// 条码。
    pub barcode: Option<String>,
    /// 基础单位 ID。
    pub base_unit_id: String,
    /// 基础单位编码。
    pub base_unit_code: Option<String>,
    /// 基础单位名称。
    pub base_unit_name: Option<String>,
    /// 公司一件代发含税参考价。
    pub sales_visible_price_gross: Amount,
    /// 公司集采含税销售参考价（独立维护，非负）。
    #[serde(default)]
    pub bulk_price_gross: Option<Amount>,
    /// 公司集采价起订数量；未维护时按一件代发价取价，有值必须大于零。
    #[serde(default)]
    pub bulk_min_quantity: Option<Quantity>,
    /// 含税市场参考价。
    pub market_price: Option<Amount>,
    /// SKU 主图文件 ID。
    pub main_image_asset_id: Option<String>,
    /// 当前商品分类；未分类为 `None`。
    #[serde(default)]
    pub category_id: Option<String>,
    /// 当前 SKU 修订生效开始日。
    pub effective_from: BusinessDate,
    /// 当前 SKU 修订生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 当前有效供给对应的去重供应商数量。
    pub supplier_count: u32,
    /// 当前有效供给对应的去重供应商稳定 ID，仅供跨域读取补齐业务编号。
    #[serde(default)]
    pub supplier_ids: Vec<String>,
    /// 按地区筛选且含运费另计时，所选地区内是否有需另付运费才能送到的地方。
    #[serde(default)]
    pub delivery_freight_extra: bool,
    /// 当前商品品牌；供预算投影按品牌筛选。
    #[serde(default)]
    pub brand_id: Option<String>,
    /// 当前商品名称；供预算投影按关键字匹配。
    #[serde(default)]
    pub product_name: Option<String>,
}

/// 公司商品池排序字段。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SellableSortField {
    /// SKU 编号。
    #[default]
    SkuNo,
    /// 一件代发价。
    DropshipPrice,
    /// 集采价。
    BulkPrice,
    /// 市场价。
    MarketPrice,
    /// SKU 集采起订数量。
    BulkMinQuantity,
    /// 有效供应商数量。
    SupplierCount,
}

impl SellableSortField {
    /// 返回排序字段在商品池行中的字段名。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 [`SellableSkuRow`] 的序列化字段名。
    ///
    /// # 错误
    /// 无。
    pub fn row_field(self) -> &'static str {
        match self {
            Self::SkuNo => "sku_no",
            Self::DropshipPrice => "sales_visible_price_gross",
            Self::BulkPrice => "bulk_price_gross",
            Self::MarketPrice => "market_price",
            Self::BulkMinQuantity => "bulk_min_quantity",
            Self::SupplierCount => "supplier_count",
        }
    }
}

/// 公司商品池排序；同值按 SKU 编号与 SKU 稳定排序。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SellableSort {
    /// 排序字段。
    pub field: SellableSortField,
    /// 是否降序。
    pub descending: bool,
}
