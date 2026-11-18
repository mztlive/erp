//! 京东资料纯转换；采购价、条码、商品分组和可供状态分别解释。
use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use erp_core::money::{Quantity, Rate, UnitPrice};
use erp_supply::entity::source_purchase::{PurchaseRestriction, SourcePurchaseTerms, SourceQuoteContext};
use erp_supply::ports::connector::common::{ConnectorResult, SupplierSku};
use erp_supply::ports::connector::offer::{
    CatalogAvailability, CatalogImage, Offer, OfferDetails, PriceTier, SupplyQuote,
};
use erp_supply::ports::connector::transform::OfferTransform;
use reqwest::Url;
use rust_decimal::Decimal;

use super::mapping;
use super::wire::{Category, SellPrice, SkuDetail, SkuImages, WireText};

/// 已读取的单 SKU 来源事实；不接收或修改公司商品及销售价。
pub struct JdVopOffer<'a> {
    pub detail: &'a SkuDetail,
    pub price: Option<&'a SellPrice>,
    pub images: Option<&'a SkuImages>,
    pub categories: &'a [Category],
    pub query_quantity: Quantity,
    pub observed_at: Instant,
}

/// 相对图片路径仅在已核实的 HTTPS 基地址下解析。
#[derive(Default)]
pub struct JdVopOfferMapper {
    pub image_base_url: Option<String>,
}

impl OfferTransform<JdVopOffer<'_>> for JdVopOfferMapper {
    fn transform(&self, source: JdVopOffer<'_>) -> ConnectorResult<Offer> {
        let detail = source.detail;
        if detail.sku_id.0 == "0" || detail.sku_name.trim().is_empty() {
            return Err(mapping("京东商品编号或名称无效"));
        }
        let spu = detail.spu_id.as_ref().filter(|value| value.0 != "0");
        let name = if spu.is_some() {
            clean(detail.spu_name.as_deref())
                .ok_or_else(|| mapping("京东SPU缺少共有名称，请先核实来源资料"))?
        } else {
            detail.sku_name.trim().to_owned()
        };

        let mut terms = purchase_limits(detail)?;
        if let Some(price) = source.price {
            terms.restrictions = price.purchase_restrictions()?;
        }

        let minimum = terms.minimum_quantity;
        let price = source.price.map(|price| self.price(detail, price)).transpose()?.flatten();
        let rate = source
            .price
            .and_then(|price| price.tax_rate_percentage.as_ref())
            .or(detail.tax_rate_percentage.as_ref());
        let tax_rate = percentage(rate)?;
        terms.quote_context = source.price.map(|_| SourceQuoteContext {
            quantity: source.query_quantity,
            observed_at: source.observed_at,
            tax_rate,
            tax_rate_raw: rate.map(|value| value.0.clone()),
            address_fingerprint: None,
        });
        let quote = price.map(|unit_price| SupplyQuote {
            dropship: Some(PriceTier { unit_price, minimum_quantity: minimum }),
            bulk: None,
            tax_rate,
            valid_until: None,
        });

        Ok(Offer {
            sku: SupplierSku {
                product_id: Some(
                    spu.map_or_else(|| format!("sku:{}", detail.sku_id.0), |spu| format!("spu:{}", spu.0)),
                ),
                spec_id: detail.sku_id.0.clone(),
            },
            name,
            specification: specification(detail),
            unit: clean(detail.sale_unit.as_deref()),
            quote,
            details: self.details(&source, terms)?,
        })
    }
}

impl JdVopOfferMapper {
    fn price(&self, detail: &SkuDetail, price: &SellPrice) -> ConnectorResult<Option<UnitPrice>> {
        if price.sku_id != detail.sku_id {
            return Err(mapping("京东报价与请求商品不一致"));
        }
        let Some(value) = price.sale_price.as_ref() else { return Ok(None) };
        let decimal: Decimal = value.0.parse().map_err(|_| mapping("京东采购价格式无效"))?;
        if decimal.is_sign_negative() {
            return Ok(None);
        }
        value.0.parse().map(Some).map_err(|_| mapping("京东采购价精度无效"))
    }

    fn details(&self, source: &JdVopOffer<'_>, terms: SourcePurchaseTerms) -> ConnectorResult<OfferDetails> {
        let detail = source.detail;
        let category_ids = detail
            .category
            .as_deref()
            .unwrap_or_default()
            .split(';')
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let category_name = category_path(&category_ids, source.categories)?;
        let images = self.images(detail, source.images)?;
        let availability = match detail.sku_state.as_ref().map(|value| value.0.as_str()) {
            Some("0") => CatalogAvailability::Unavailable,
            Some("1") | None => CatalogAvailability::Unknown,
            _ => return Err(mapping("京东上下架状态未识别")),
        };
        Ok(OfferDetails {
            category_id: category_ids.last().map(|value| (*value).to_owned()),
            category_name,
            brand_name: clean(detail.brand_name.as_deref()),
            images,
            availability,
            source_purchase_terms: Some(terms),
            ..OfferDetails::default()
        })
    }

    fn images(
        &self,
        detail: &SkuDetail,
        images: Option<&SkuImages>,
    ) -> ConnectorResult<Option<Vec<CatalogImage>>> {
        let Some(images) = images else {
            return Ok(detail.image_path.as_deref().map(|path| vec![self.image(path)]));
        };
        if images.sku_id != detail.sku_id {
            return Err(mapping("京东图片与请求商品不一致"));
        }
        let mut values = images.sku_image_list.iter().collect::<Vec<_>>();
        if images.sku_image_list.iter().any(|image| {
            !matches!(image.yn.0.as_str(), "0" | "1") || !matches!(image.is_primary.0.as_str(), "0" | "1")
        }) {
            return Err(mapping("京东图片有效性标记未识别"));
        }
        values.sort_by_key(|image| (image.is_primary.0 != "1", image.order_sort.0.parse::<u64>().ok()));
        values
            .into_iter()
            .map(|image| {
                let mut mapped = self.image(&image.short_path);
                mapped.source_primary = Some(image.is_primary.0 == "1");
                mapped.source_valid = Some(image.yn.0 == "1");
                mapped.source_sequence =
                    Some(image.order_sort.0.parse().map_err(|_| mapping("京东图片顺序格式无效"))?);
                Ok(mapped)
            })
            .collect::<ConnectorResult<Vec<_>>>()
            .map(Some)
    }

    fn image(&self, path: &str) -> CatalogImage {
        let absolute = if path.starts_with("//") { format!("https:{path}") } else { path.to_owned() };
        let url = Url::parse(&absolute).ok().or_else(|| {
            if path.starts_with('/') || path.contains("..") || path.contains(':') {
                return None;
            }
            Url::parse(self.image_base_url.as_deref()?).ok()?.join(path).ok()
        });
        let url = url
            .filter(|url| {
                matches!(url.scheme(), "https" | "http")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none()
                    && url.as_str().len() <= 2048
            })
            .map(|url| url.to_string())
            .unwrap_or_default();
        CatalogImage { url, source_path: Some(path.to_owned()), ..CatalogImage::default() }
    }
}

impl SellPrice {
    /// 将需要额外用户资格或促销限购核验的来源报价标记为受限。
    /// # 参数
    /// 无；读取本次按量报价中的促销事实。
    /// # 返回
    /// 普通报价返回空限制，一口价及未支持的促销分别保留限制原因。
    /// # 错误
    /// 已提供但无法识别的促销标记返回映射错误，不猜测为普通价格。
    pub(super) fn purchase_restrictions(&self) -> ConnectorResult<Vec<PurchaseRestriction>> {
        let promotional = match self.has_promotion.as_ref().map(|value| value.0.as_str()) {
            Some("true") => true,
            Some("false") | None => false,
            _ => return Err(mapping("京东报价促销状态未识别，请先核实来源价格")),
        };
        let kind = self.promotion_type.as_ref().map(|value| value.0.as_str());
        if kind == Some("2") || self.fixed_price_promotion.is_some() {
            return Ok(vec![PurchaseRestriction::PersonalizedPrice]);
        }
        let unverified_kind = kind.is_some_and(|kind| kind != "1");
        if promotional || unverified_kind || (kind == Some("1") && self.has_promotion.is_none()) {
            return Ok(vec![PurchaseRestriction::OtherRequiredProcess {
                code: "JD_VOP_PROMOTION_LIMITS_REQUIRED".into(),
            }]);
        }
        Ok(vec![])
    }
}

/// 将详情中已经核实的来源数量限制转换为通用采购约束。
/// # 参数
/// `detail` 为请求了购买倍数扩展的本次商品详情。
/// # 返回
/// 起购量及倍数；来源缺失时分别保留未知。
/// # 错误
/// 来源数量不是正数或超出精度时拒绝，不猜测默认一件。
pub(super) fn purchase_limits(detail: &SkuDetail) -> ConnectorResult<SourcePurchaseTerms> {
    Ok(SourcePurchaseTerms {
        minimum_quantity: quantity(detail.lowest_buy.as_ref())?,
        quantity_multiple: quantity(
            detail.sku_ext_map.as_ref().and_then(|extensions| extensions.quantity_multiple.as_ref()),
        )?,
        ..Default::default()
    })
}

fn quantity(value: Option<&WireText>) -> ConnectorResult<Option<Quantity>> {
    let Some(value) = value else { return Ok(None) };
    let result: Quantity = value.0.parse().map_err(|_| mapping("京东来源采购数量限制格式无效"))?;
    if result.to_decimal() <= Decimal::ZERO {
        return Err(mapping("京东来源采购数量限制必须为正"));
    }
    Ok(Some(result))
}

pub(super) fn percentage(value: Option<&WireText>) -> ConnectorResult<Option<Rate>> {
    let Some(value) = value else { return Ok(None) };
    let percentage: Decimal = value.0.parse().map_err(|_| mapping("京东来源税率格式无效"))?;
    if !(Decimal::ZERO..=Decimal::ONE_HUNDRED).contains(&percentage) {
        return Err(mapping("京东来源税率超出范围"));
    }
    let rate = percentage.checked_div(Decimal::ONE_HUNDRED).ok_or_else(|| mapping("京东来源税率精度无效"))?;
    rate.to_string().parse().map(Some).map_err(|_| mapping("京东来源税率精度无效"))
}

fn clean(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned)
}

fn specification(detail: &SkuDetail) -> String {
    let mut values = Vec::new();
    for (label, value) in [
        ("型号", detail.seo_model.as_deref()),
        ("颜色", detail.color.as_deref()),
        ("尺寸", detail.size.as_deref()),
        ("容量", detail.capacity.as_deref()),
        ("包装", detail.ware_info.as_deref()),
    ] {
        if let Some(value) = clean(value) {
            values.push(format!("{label}：{value}"))
        }
    }

    for group in &detail.param_group_attr_list {
        for attr in &group.param_attribute_list {
            let name = attr.param_attr_name.trim();
            let value = attr
                .param_attr_val_list
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join("、");
            if !name.is_empty() && !value.is_empty() {
                values.push(format!("{} / {name}：{value}", group.param_group_name.trim()))
            }
        }
    }

    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
    if values.is_empty() { detail.sku_name.trim().to_owned() } else { values.join("；") }
}

fn category_path(ids: &[&str], categories: &[Category]) -> ConnectorResult<Option<String>> {
    if ids.is_empty() {
        return Ok(None);
    }
    if ids.iter().any(|id| !id.bytes().all(|byte| byte.is_ascii_digit())) {
        return Err(mapping("京东类目路径格式无效"));
    }
    let mut names = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let Some(category) = categories.iter().find(|category| category.category_id.0 == *id) else {
            return Ok(None);
        };
        if index > 0 && category.parent_id.0 != ids[index - 1] {
            return Err(mapping("京东类目父子关系不一致"));
        }
        names.push(category.category_name.trim());
    }
    Ok(Some(names.join(" / ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail() -> SkuDetail {
        serde_json::from_str(r#"{"skuId":9007199254740993,"skuName":"蓝色大号","spuId":"0","saleUnit":"包","seoModel":"M1","color":"蓝色","size":"XL","lowestBuy":"2","skuState":"1"}"#).unwrap()
    }

    fn mapped(detail: &SkuDetail, price: Option<&SellPrice>) -> Offer {
        JdVopOfferMapper::default()
            .transform(JdVopOffer {
                detail,
                price,
                images: None,
                categories: &[],
                query_quantity: "1".parse().unwrap(),
                observed_at: Instant::from_unix_secs(1),
            })
            .unwrap()
    }

    #[test]
    fn identity_and_order_unit_are_honest_without_barcode_or_inventory_inference() {
        let mut detail = detail();
        let offer = mapped(&detail, None);
        assert_eq!(offer.sku.product_id.as_deref(), Some("sku:9007199254740993"));
        assert_eq!(offer.sku.spec_id, "9007199254740993");
        assert_eq!(offer.unit.as_deref(), Some("包"));
        assert!(offer.specification.contains("蓝色"));
        assert!(offer.details.barcode.is_none());
        assert_eq!(offer.details.availability, CatalogAvailability::Unknown);
        assert!(offer.details.available_quantity.is_none());

        detail.spu_id = Some(super::super::wire::WireId("123".into()));
        detail.spu_name = Some("共有名称".into());
        assert_eq!(mapped(&detail, None).name, "共有名称");
    }

    #[test]
    fn actual_sale_price_preserves_zero_and_missing_and_source_percentage() {
        let detail = detail();
        for (price, expected) in [("0", Some("0")), ("-1", None), ("55.0600", Some("55.06"))] {
            let price: SellPrice = serde_json::from_str(&format!(
                r#"{{"skuId":"9007199254740993","salePrice":"{price}","taxRatePercentage":"9"}}"#
            ))
            .unwrap();
            let offer = mapped(&detail, Some(&price));
            let unit_price =
                offer.quote.as_ref().and_then(|quote| quote.dropship.as_ref()).map(|tier| tier.unit_price);
            assert_eq!(unit_price, expected.map(|value| value.parse::<UnitPrice>().unwrap()));
            assert_eq!(
                offer
                    .details
                    .source_purchase_terms
                    .unwrap()
                    .quote_context
                    .unwrap()
                    .tax_rate
                    .unwrap()
                    .to_string(),
                "0.09"
            );
        }
        assert!(mapped(&detail, None).quote.is_none());
    }

    #[test]
    fn images_require_safe_source_urls_and_explicit_relative_base() {
        let mapper = JdVopOfferMapper::default();
        assert_eq!(mapper.image("file:///private/secret").url, "");
        assert_eq!(mapper.image("jfs/a.jpg").url, "");
        assert_eq!(mapper.image("//img.example/a.jpg").url, "https://img.example/a.jpg");

        let mapper = JdVopOfferMapper { image_base_url: Some("https://img.example/base/".into()) };
        assert_eq!(mapper.image("jfs/a.jpg").url, "https://img.example/base/jfs/a.jpg");
        assert!(mapper.image("../private").url.is_empty());
    }

    #[test]
    fn official_multiple_extension_maps_and_checks_actual_quantity_without_guessing() {
        let mut detail = detail();
        assert!(mapped(&detail, None).details.source_purchase_terms.unwrap().quantity_multiple.is_none());
        for raw in [r#"{"sfbsgm":4}"#, r#"{"key":"sfbsgm","value":"4"}"#] {
            detail.sku_ext_map = Some(serde_json::from_str(raw).unwrap());
            let terms = mapped(&detail, None).details.source_purchase_terms.unwrap();
            assert_eq!(terms.quantity_multiple, Some("4".parse().unwrap()));
            terms.ensure_quantity("8".parse().unwrap()).unwrap();
            assert!(terms.ensure_quantity("6".parse().unwrap()).is_err());
        }
        for value in ["0", "-1", "abc"] {
            detail.sku_ext_map = Some(serde_json::from_str(&format!(r#"{{"sfbsgm":"{value}"}}"#)).unwrap());
            assert_eq!(purchase_limits(&detail).unwrap_err().code, "JD_VOP_MAPPING_INVALID");
        }
        assert!(
            serde_json::from_str::<SkuDetail>(
                r#"{"skuId":"1","skuName":"x","skuExtMap":{"sfbsgm":"2","key":"sfbsgm","value":"4"}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn promotional_prices_keep_source_restrictions_instead_of_becoming_ordinary_offers() {
        let detail = detail();
        for (flags, expected) in [
            (r#""hasPromotion":true,"promotionType":2"#, PurchaseRestriction::PersonalizedPrice),
            (
                r#""fixedPricePromotion":{"limitedNum":2,"remainNum":1}"#,
                PurchaseRestriction::PersonalizedPrice,
            ),
            (
                r#""hasPromotion":true,"promotionType":1"#,
                PurchaseRestriction::OtherRequiredProcess { code: "JD_VOP_PROMOTION_LIMITS_REQUIRED".into() },
            ),
        ] {
            let price: SellPrice =
                serde_json::from_str(&format!(r#"{{"skuId":"9007199254740993","salePrice":"10",{flags}}}"#,))
                    .unwrap();
            let offer = mapped(&detail, Some(&price));
            assert_eq!(offer.quote.unwrap().dropship.unwrap().unit_price, "10".parse().unwrap());
            let terms = offer.details.source_purchase_terms.unwrap();
            assert_eq!(terms.restrictions, vec![expected]);
            assert!(terms.ensure_automatic_order().is_err());
        }

        let price: SellPrice =
            serde_json::from_str(r#"{"skuId":"1","salePrice":"10","hasPromotion":"UNKNOWN"}"#).unwrap();
        assert!(price.purchase_restrictions().is_err());
        let ordinary: SellPrice =
            serde_json::from_str(r#"{"skuId":"1","salePrice":"10","hasPromotion":false,"promotionType":1}"#)
                .unwrap();
        assert!(ordinary.purchase_restrictions().unwrap().is_empty());
    }
}
