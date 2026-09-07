//! 可售查询消费的筛选条件与稳定行合同。
use crate::entity::catalog::ProductKind;
use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};

/// 公司商品池列表筛选条件。
///
/// 资格硬条件（启用、销售可见价、有效供给等）由聚合管道固定施加；本结构只承载
/// 调用方可选的业务筛选。供应商身份仅用于筛选匹配，不会写入投影行。
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
    /// 当前有效供给可供区域（精确匹配并集中的任一区域）；`None` 表示不筛选。
    pub supply_region: Option<String>,
    /// 当前有效供给去重供应商数量上限（含）；`None` 表示不按供应保障筛选。
    pub max_supplier_count: Option<u32>,
    /// 销售可见含税价下限（含）；`None` 表示无下限。
    pub sales_price_min: Option<Amount>,
    /// 销售可见含税价上限（含）；`None` 表示无上限。
    pub sales_price_max: Option<Amount>,
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
            supply_region: None,
            max_supplier_count: None,
            sales_price_min: None,
            sales_price_max: None,
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
    /// 公司销售可见含税价。
    pub sales_visible_price_gross: Amount,
    /// 市场参考价。
    pub market_price: Option<Amount>,
    /// SKU 主图文件 ID。
    pub main_image_asset_id: Option<String>,
    /// 当前 SKU 修订生效开始日。
    pub effective_from: BusinessDate,
    /// 当前 SKU 修订生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 当前有效供给对应的去重供应商数量。
    pub supplier_count: u32,
    /// 当前有效供给的可供区域并集。
    #[serde(default)]
    pub supply_regions: Vec<String>,
}
