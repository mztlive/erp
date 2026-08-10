//! 公司商品池只读查询。
//!
//! 公司商品池不是独立聚合根；本模块只把公司稳定 SKU、当前 SKU 修订与当前
//! 有效供给组合为销售只读投影。资格判定由 catalog Repository 的同一条聚合
//! 管道执行，销售单提交也复用该仓储判定。

use database::{CatalogExt, NoTransaction};
use entities::catalog::ProductKind;
use entities::common::time::BusinessDate;
use entities::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{CatalogService, PageView};
use crate::errors::{Error, Result};

/// 公司商品池列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SellableSkuListParams {
    /// SKU 编码、SKU 名称或商品编码的字面量搜索。
    pub q: Option<String>,
    /// 商品业务类型筛选。
    pub product_kind: Option<ProductKind>,
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

/// 将稳定 SKU 规格签名转换为对外的结构化规格属性。
fn specification_attributes(signature: &str) -> Vec<SellableSkuSpecificationAttributeView> {
    signature
        .split('|')
        .filter_map(|entry| {
            let (name, value) = entry.split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            (!name.is_empty() && !value.is_empty()).then(|| SellableSkuSpecificationAttributeView {
                name: name.to_string(),
                value: value.to_string(),
            })
        })
        .collect()
}

impl CatalogService {
    /// 分页查询符合销售资格的公司 SKU。
    ///
    /// # 参数
    /// * `params` - 搜索、商品类型、资格日期与分页参数
    ///
    /// # 返回
    /// 返回只读公司商品池分页视图；不包含任何采购成本或供应商身份。
    ///
    /// # 错误
    /// 参数非法时返回 `ValidationError`；聚合查询失败时返回数据库错误。
    pub async fn sellable_sku_list(
        &self,
        params: &SellableSkuListParams,
    ) -> Result<PageView<SellableSkuView>> {
        params.validate()?;
        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20);
        let eligibility_as_of = params.eligibility_as_of.unwrap_or_else(BusinessDate::today);
        let keyword = params
            .q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let rows = self
            .db
            .catalog()
            .search_sellable_skus(
                keyword,
                params.product_kind,
                eligibility_as_of,
                page,
                page_size,
                &mut NoTransaction,
            )
            .await?;
        let items = rows
            .items
            .into_iter()
            .map(|row| SellableSkuView {
                sku_id: row.sku_id,
                sku_version: row.sku_version,
                sku_revision_id: row.sku_revision_id,
                sku_revision_no: row.sku_revision_no,
                sku_no: row.sku_no,
                product_id: row.product_id,
                product_no: row.product_no,
                product_kind: row.product_kind,
                name: row.name,
                specification_attributes: specification_attributes(&row.specification_signature),
                specification: row.specification,
                barcode: row.barcode,
                base_unit_id: row.base_unit_id,
                base_unit_code: row.base_unit_code,
                base_unit_name: row.base_unit_name,
                sales_visible_price_gross: row.sales_visible_price_gross,
                market_price: row.market_price,
                main_image_asset_id: row.main_image_asset_id,
                effective_from: row.effective_from,
                effective_to: row.effective_to,
                supplier_count: row.supplier_count,
                supply_regions: row.supply_regions,
                eligibility_as_of,
            })
            .collect();
        Ok(PageView {
            items,
            total: rows.total,
            page,
            page_size,
        })
    }
}

/// 构造销售资格失效错误。
///
/// # 参数
/// * `sku_ids` - 已失效或修订已变化的稳定 SKU ID 集合
///
/// # 返回
/// 返回可直接向业务调用方暴露的 fail-closed 错误。
pub(crate) fn sellable_sku_invalid_error(sku_ids: &[String]) -> Error {
    Error::BusinessLogicError(format!(
        "销售商品已失效或修订已变化，请刷新公司商品池后重试: {}",
        sku_ids.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::{specification_attributes, SellableSkuListParams};
    use validator::Validate;

    /// 公司商品池分页上限固定为一百，阻止无界销售查询。
    #[test]
    fn sellable_sku_page_size_is_bounded() {
        let params = SellableSkuListParams {
            q: None,
            product_kind: None,
            eligibility_as_of: None,
            page: Some(1),
            page_size: Some(101),
        };

        assert!(params.validate().is_err());
    }

    /// 公司商品池返回真实规格属性名/值，无规格 SKU 返回空集合。
    #[test]
    fn sellable_sku_specification_attributes_come_from_stable_identity() {
        let attributes = specification_attributes("尺码=L|颜色=红色");

        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].name, "尺码");
        assert_eq!(attributes[0].value, "L");
        assert_eq!(attributes[1].name, "颜色");
        assert_eq!(attributes[1].value, "红色");
        assert!(specification_attributes("").is_empty());
    }
}
