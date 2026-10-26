//! 蛋糕叔叔商品转换；列表、详情与后台采集共用同一入口。
use erp_core::money::UnitPrice;
use erp_supply::ports::connector::common::{ConnectorResult, SupplierSku};
use erp_supply::ports::connector::offer::{Offer, PriceTier, SupplyQuote};
use erp_supply::ports::connector::transform::OfferTransform;
use rust_decimal::Decimal;
use serde_json::Value;

use super::DangaoshushuConnector;
use super::parsing::{field, fixed, id, mapping};

/// 一个待转换的供应商规格；借用响应，不复制原始 JSON。
pub enum DangaoshushuOffer<'a> {
    /// 列表商品及其规格，规格身份来自 `spec_id`。
    List { product: &'a Value, spec: &'a Value },
    /// 完整详情响应及已定位的规格，规格身份来自 `id`。
    Detail { root: &'a Value, spec: &'a Value },
}

impl OfferTransform<DangaoshushuOffer<'_>> for DangaoshushuConnector {
    fn transform(&self, source: DangaoshushuOffer<'_>) -> ConnectorResult<Offer> {
        let (product, spec, spec_id_field, root) = match source {
            DangaoshushuOffer::List { product, spec } => (product, spec, "spec_id", None),
            DangaoshushuOffer::Detail { root, spec } => (&root["product"], spec, "id", Some(root)),
        };
        let spec_id = id(&spec[spec_id_field])?;
        let unit = self.settings.spec_units.get(&spec_id).cloned();
        let quote = if !spec["clearing_price"].is_null() {
            let unit_price: UnitPrice = fixed(&spec["clearing_price"])?;
            if unit_price.to_decimal() < Decimal::ZERO {
                return Err(mapping());
            }
            Some(SupplyQuote {
                dropship: Some(PriceTier {
                    unit_price,
                    minimum_quantity: "1".parse().map_err(|_| mapping())?,
                }),
                bulk: None,
                tax_rate: None,
                valid_until: None,
            })
        } else {
            None
        };
        Ok(Offer {
            sku: SupplierSku { product_id: Some(id(&product["id"])?), spec_id },
            name: display_field(product, "title")?,
            specification: display_field(spec, "name")?,
            unit,
            quote,
            details: super::catalog_details::details(product, spec, root)?,
        })
    }
}

/// 展示名称允许供应商附带首尾空白，身份和金额继续使用严格解析。
fn display_field(value: &Value, key: &str) -> ConnectorResult<String> {
    let result = match value[key].as_str() {
        Some(value) if !value.trim().is_empty() => Ok(value.trim().to_owned()),
        _ => field(value, key),
    };
    result.map_err(|mut failure| {
        failure.summary = format!("供应商名称字段 {key} 缺失、为空或类型无效");
        failure
    })
}

#[cfg(test)]
mod tests {
    use erp_supply::entity::failure::SupplierFailureClass;
    use serde_json::json;

    use super::super::test_support::recording_connector;
    use super::*;

    #[test]
    fn transform_unifies_list_and_detail_without_external_calls() {
        let (connector, transport) = recording_connector(vec![]);
        let product = json!({"id":"00023","title":" 蛋糕 ","cat_id":"12"});
        let spec = json!({"spec_id":"spec-1","name":" 6寸 ","clearing_price":"100.25"});
        let listed = connector.transform(DangaoshushuOffer::List { product: &product, spec: &spec }).unwrap();
        let root = json!({
            "product":product,
            "images":[{"m_path":"https://img.dangaoss.com/cake.jpg"}],
            "content_images":[],
            "brand":{"name":"品牌"}
        });
        let spec = json!({"id":"spec-1","name":" 6寸 ","clearing_price":"100.25","stock":"2"});
        let mapper: &dyn OfferTransform<DangaoshushuOffer<'_>> = &connector;
        let detailed = mapper.transform(DangaoshushuOffer::Detail { root: &root, spec: &spec }).unwrap();
        assert_eq!(listed.sku, detailed.sku);
        assert_eq!(detailed.sku.product_id.as_deref(), Some("00023"));
        assert_eq!(listed.name, "蛋糕");
        assert_eq!(listed.name, detailed.name);
        assert_eq!(listed.specification, detailed.specification);
        assert_eq!(listed.unit, detailed.unit);
        assert_eq!(listed.quote, detailed.quote);
        assert!(listed.details.images.is_none());
        assert_eq!(detailed.details.images.unwrap()[0].url, "https://img.dangaoss.com/cake.jpg");
        assert_eq!(detailed.details.detail_images, Some(vec![]));
        assert_eq!(detailed.details.brand_name.as_deref(), Some("品牌"));
        assert_eq!(detailed.details.available_quantity.as_deref(), Some("2"));
        assert!(transport.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn transform_rejects_invalid_detail_media_and_preserves_first_error() {
        let (connector, transport) = recording_connector(vec![]);
        let root = json!({
            "product":{"id":"p1","title":"商品"},
            "images":[{"m_path":"file:///private/image"}]
        });
        let spec = json!({"id":"s1","name":"规格"});
        let error = connector.transform(DangaoshushuOffer::Detail { root: &root, spec: &spec }).unwrap_err();
        assert_eq!(error.class, SupplierFailureClass::MappingError);
        assert_eq!(error.code, "DGSS_MAPPING");
        let spec = json!({"id":"s1","name":" "});
        let error = connector.transform(DangaoshushuOffer::Detail { root: &root, spec: &spec }).unwrap_err();
        assert_eq!(error.summary, "供应商名称字段 name 缺失、为空或类型无效");
        assert!(transport.requests.lock().unwrap().is_empty());
    }
}
