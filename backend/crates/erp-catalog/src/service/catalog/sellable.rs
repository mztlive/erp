//! 公司商品池只读查询。
//!
//! 公司商品池不是独立聚合根；本模块只把公司稳定 SKU、当前 SKU 修订与当前
//! 有效供给组合为销售只读投影。资格判定由 catalog Repository 的同一条聚合
//! 管道执行，销售单提交也复用该仓储判定。

use application_core::normalized_text;
use erp_core::common::time::BusinessDate;
use erp_core::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::PageView;
use crate::dto::validate_sales_price_range;
use crate::entity::catalog::ProductKind;
use crate::error::{Error, Result};
use crate::repository::CatalogExt;

/// 公司商品池列表筛选条件类型。
type SellableSkuFilter = <mongodb::Database as CatalogExt>::SellableSkuFilter;

/// 公司商品池列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct SellableSkuListParams {
    /// SKU 编码、SKU 名称、商品编码/名称、规格或条码的字面量搜索。
    pub q: Option<String>,
    /// 商品业务类型筛选。
    pub product_kind: Option<ProductKind>,
    /// 当前商品分类筛选。
    pub category_id: Option<String>,
    /// 当前商品品牌筛选。
    pub brand_id: Option<String>,
    /// 当前有效供给中的供应商筛选；响应仅返回供应商业务编号。
    pub supplier_id: Option<String>,
    /// 当前有效供给可供区域筛选（精确匹配）。
    pub supply_region: Option<String>,
    /// 当前有效供给去重供应商数量上限（含）；用于「单一供应商」快捷视图。
    #[validate(range(min = 1, message = "供应商数量上限必须大于0"))]
    pub max_supplier_count: Option<u32>,
    /// 一件代发含税参考价下限（含）。
    pub sales_price_min: Option<Amount>,
    /// 一件代发含税参考价上限（含）。
    pub sales_price_max: Option<Amount>,
    /// 服务端解释的资格业务日期；空表示服务端今天。
    pub eligibility_as_of: Option<BusinessDate>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
}

/// 公司商品池销售只读行。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SellableSkuView {
    /// 稳定 SKU ID；公司商品池不生成独立池条目 ID。
    pub sku_id: String,
    /// 稳定 SKU 乐观锁版本。
    pub sku_version: u64,
    /// 当前且符合资格的精确 SKU 修订 ID。
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
    /// 稳定 SKU 身份对应的规格属性名与取值。
    pub specification_attributes: Vec<SellableSkuSpecificationAttributeView>,
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
    /// 公司出厂含税销售参考价（独立维护，非负）。
    #[serde(default)]
    pub factory_price_gross: Option<Amount>,
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
    /// 当前 SKU 修订生效开始日。
    pub effective_from: BusinessDate,
    /// 当前 SKU 修订生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 当前有效供给对应的去重供应商数量。
    pub supplier_count: u32,
    /// 当前有效供给对应的去重供应商业务编号，由跨域读取批量补齐。
    pub supplier_codes: Vec<String>,
    /// 当前有效供给可供区域并集。
    pub supply_regions: Vec<String>,
    /// 本次资格判定的服务端业务日期。
    pub eligibility_as_of: BusinessDate,
}

/// 公司商品池中一项 SKU 规格属性。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SellableSkuSpecificationAttributeView {
    /// SPU 内的规格属性名。
    pub name: String,
    /// 当前 SKU 选中的规格属性值。
    pub value: String,
}

/// 将稳定 SKU 规格签名映射为对外的结构化规格属性。
///
/// 规范签名由 [`crate::entity::catalog::read_specification_signature`] 解析；历史非法
/// 签名在审计清零前兼容为空属性，避免列表整页失败。
///
/// # 参数
/// * `signature` - 已持久化的规格签名
///
/// # 返回
/// 返回按规范顺序排列的规格属性视图；非法历史签名返回空集合。
///
/// # 错误
/// 无。
fn specification_attribute_views(signature: &str) -> Vec<SellableSkuSpecificationAttributeView> {
    match crate::entity::catalog::read_specification_signature(signature) {
        crate::entity::catalog::SpecificationSignatureRead::Canonical(entries) => entries
            .into_iter()
            .map(|entry| SellableSkuSpecificationAttributeView {
                name: entry.attribute_code,
                value: entry.value_code,
            })
            .collect(),
        crate::entity::catalog::SpecificationSignatureRead::LegacyNonCanonical => Vec::new(),
    }
}

/// 构造销售资格失效错误。
///
/// # 参数
/// * `sku_ids` - 已失效或修订已变化的稳定 SKU ID 集合
///
/// # 返回
/// 返回可直接向业务调用方暴露的 fail-closed 错误。
pub fn sellable_sku_invalid_error(sku_ids: &[String]) -> Error {
    Error::BusinessLogicError(format!(
        "销售商品已失效或修订已变化，请刷新公司商品池后重试: {}",
        sku_ids.join(", ")
    ))
}

/// 校验分页、价格并在原时点解释资格日期。
pub fn prepare_sellable_sku_list(params: &SellableSkuListParams) -> Result<SellableSkuFilter> {
    use application_core::{page_or_default, page_size_or_default};

    params.validate()?;
    validate_sales_price_range(params.sales_price_min, params.sales_price_max)?;
    let page = page_or_default(params.page);
    let page_size = page_size_or_default(params.page_size);
    let eligibility_as_of = params.eligibility_as_of.unwrap_or_else(BusinessDate::today);
    Ok(SellableSkuFilter {
        nationwide_only: false,
        keyword: normalized_text(params.q.as_deref()),
        product_kind: params.product_kind,
        category_id: normalized_text(params.category_id.as_deref()),
        brand_id: normalized_text(params.brand_id.as_deref()),
        supplier_id: normalized_text(params.supplier_id.as_deref()),
        supply_region: normalized_text(params.supply_region.as_deref()),
        max_supplier_count: params.max_supplier_count,
        sales_price_min: params.sales_price_min,
        sales_price_max: params.sales_price_max,
        eligibility_as_of,
        page,
        page_size,
    })
}

/// 投影可售行，保留历史非法规格签名为空属性的合同。
pub fn sellable_sku_page_view(
    rows: persistence_core::PageResult<crate::repository::SellableSkuRow>,
    filter: &SellableSkuFilter,
) -> Result<PageView<SellableSkuView>> {
    let page = filter.page;
    let page_size = filter.page_size;
    let eligibility_as_of = filter.eligibility_as_of;
    let mut items = Vec::with_capacity(rows.items.len());
    for row in rows.items {
        items.push(SellableSkuView {
            sku_id: row.sku_id,
            sku_version: row.sku_version,
            sku_revision_id: row.sku_revision_id,
            sku_revision_no: row.sku_revision_no,
            sku_no: row.sku_no,
            product_id: row.product_id,
            product_no: row.product_no,
            product_kind: row.product_kind,
            name: row.name,
            specification_attributes: specification_attribute_views(&row.specification_signature),
            specification: row.specification,
            barcode: row.barcode,
            base_unit_id: row.base_unit_id,
            base_unit_code: row.base_unit_code,
            base_unit_name: row.base_unit_name,
            factory_price_gross: row.factory_price_gross,
            sales_visible_price_gross: row.sales_visible_price_gross,
            bulk_price_gross: row.bulk_price_gross,
            bulk_min_quantity: row.bulk_min_quantity,
            market_price: row.market_price,
            main_image_asset_id: row.main_image_asset_id,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            supplier_count: row.supplier_count,
            supplier_codes: Vec::new(),
            supply_regions: row.supply_regions,
            eligibility_as_of,
        });
    }
    Ok(PageView { items, total: rows.total, page, page_size })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::money::{Amount, Quantity};
    use persistence_core::PageResult;
    use serde_json::json;
    use validator::Validate;

    use super::{SellableSkuListParams, sellable_sku_page_view, specification_attribute_views};
    use crate::repository::{SellableSkuFilter, SellableSkuRow};

    /// 商品池投影保留四种独立参考价与起订量，供应商内部身份不对外序列化。
    #[test]
    fn sellable_view_preserves_reference_prices_without_internal_supplier_ids() {
        let row: SellableSkuRow = serde_json::from_value(json!({
            "sku_id": "sku-1", "sku_version": 1,
            "sku_revision_id": "rev-1", "sku_revision_no": 1, "sku_no": "SKU-01",
            "product_id": "product-1", "product_no": "P-01", "product_kind": "PHYSICAL",
            "name": "礼盒", "specification_signature": "", "base_unit_id": "unit-1",
            "factory_price_gross": "70.00", "sales_visible_price_gross": "99.90",
            "bulk_price_gross": "80.00", "bulk_min_quantity": "10.000000", "market_price": "129.00",
            "effective_from": "2026-01-01", "supplier_count": 1, "supplier_ids": ["supplier-1"]
        }))
        .unwrap();
        let filter = SellableSkuFilter::as_of(BusinessDate::from_ymd(2026, 1, 1).unwrap());
        let page = sellable_sku_page_view(PageResult { items: vec![row], total: 1 }, &filter).unwrap();
        let item = &page.items[0];
        assert_eq!(item.factory_price_gross, Some(Amount::from_str("70.00").unwrap()));
        assert_eq!(item.sales_visible_price_gross, Amount::from_str("99.90").unwrap());
        assert_eq!(item.bulk_price_gross, Some(Amount::from_str("80.00").unwrap()));
        assert_eq!(item.bulk_min_quantity, Some(Quantity::from_str("10.000000").unwrap()));
        assert_eq!(item.market_price, Some(Amount::from_str("129.00").unwrap()));
        assert!(item.supplier_codes.is_empty());
        let wire = serde_json::to_value(item).unwrap();
        assert!(wire.get("supplier_ids").is_none());
    }

    /// 公司商品池分页上限固定为一百，阻止无界销售查询。
    #[test]
    fn sellable_sku_page_size_is_bounded() {
        let params = SellableSkuListParams { page: Some(1), page_size: Some(101), ..Default::default() };

        assert!(params.validate().is_err());
    }

    /// 供应商数量上限必须为正整数，避免把无供给 SKU 误当成筛选条件。
    #[test]
    fn sellable_sku_max_supplier_count_rejects_zero() {
        let params = SellableSkuListParams {
            max_supplier_count: Some(0),
            page: Some(1),
            page_size: Some(20),
            ..Default::default()
        };

        assert!(params.validate().is_err());
    }

    /// 公司商品池只映射规范签名；无规格 SKU 返回空集合。
    #[test]
    fn sellable_sku_specification_attributes_come_from_stable_identity() {
        let attributes = specification_attribute_views("尺码=L|颜色=红色");

        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].name, "尺码");
        assert_eq!(attributes[0].value, "L");
        assert_eq!(attributes[1].name, "颜色");
        assert_eq!(attributes[1].value, "红色");
        assert!(specification_attribute_views("").is_empty());
        assert!(specification_attribute_views("尺码L").is_empty());
    }
}
