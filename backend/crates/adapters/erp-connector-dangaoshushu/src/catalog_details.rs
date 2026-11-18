//! 官方新版列表与详情字段归一；不将图片地址或来源编号冒充内容指纹。
use erp_core::money::Amount;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::offer::{
    AvailabilityState, AvailableQuantity, CatalogAvailability, CatalogImage, OfferDetails,
};
use reqwest::Url;
use serde_json::Value;

use super::catalog::{availability_state, stock};
use super::parsing::{context, fixed, id, mapping};

// 列表和详情转为来源报价时都从这里提取品牌、分类、条码与市场价；有完整详情才补图片和库存。
// 品牌、商品、规格状态不齐时保留未知可供性，不用 is_up 补齐；无效标识、状态或金额仍按映射错误处理。
pub(super) fn details(product: &Value, spec: &Value, root: Option<&Value>) -> ConnectorResult<OfferDetails> {
    let category_id = optional_id(product, "cat_id3")?
        .or(optional_id(product, "cat_id2")?)
        .or(optional_id(product, "cat_id")?);
    let mut details = OfferDetails {
        barcode: optional_text(spec, "barcode")?,
        market_price: market_price(spec)?,
        category_id,
        category_name: None, // 由独立分类字典补齐；label_name 是商品标签，不得冒充分类。
        brand_id: optional_id(product, "brand_id")?,
        brand_name: optional_text(product, "brand_name")?,
        description: optional_text(product, "description")?,
        ..OfferDetails::default()
    };

    if let Some(root) = root {
        details.images = image_list(root, "images");
        details.detail_images = image_list(root, "content_images");
        details.brand_name = details.brand_name.or(optional_text(&root["brand"], "name")?);

        let complete_state = !root["brand"]["status"].is_null()
            && !root["product"]["status"].is_null()
            && !root["product"]["can_buy"].is_null()
            && !spec["deleted"].is_null()
            && !spec["can_buy"].is_null();
        let quantity = stock(&spec["stock"]).map_err(|e| context(e, "供应商提供的规格库存无效"))?;
        details.availability = if complete_state {
            match availability_state(root, spec, &quantity)? {
                AvailabilityState::Available => CatalogAvailability::Available,
                AvailabilityState::Unavailable { .. } => CatalogAvailability::Unavailable,
                AvailabilityState::Unknown => CatalogAvailability::Unknown,
            }
        } else {
            CatalogAvailability::Unknown
        };
        if let AvailableQuantity::Exact(quantity) = quantity {
            details.available_quantity = Some(quantity.to_string());
        }
    }
    Ok(details)
}

fn optional_id(value: &Value, key: &str) -> ConnectorResult<Option<String>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.is_empty() || value == "0" => Ok(None),
        Some(Value::Number(value)) if value.as_u64() == Some(0) => Ok(None),
        Some(value) => id(value).map(Some).map_err(|e| {
            let label = if key == "brand_id" { "品牌编号" } else { "商品分类编号" };
            context(e, &format!("供应商提供的{label}无效"))
        }),
    }
}

fn optional_text(value: &Value, key: &str) -> ConnectorResult<Option<String>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.trim().to_owned())),
        _ => {
            let label = match key {
                "barcode" => "条码",
                "brand_name" | "name" => "品牌名称",
                _ => "商品说明",
            };
            Err(context(mapping(), &format!("供应商提供的{label}无法读取")))
        },
    }
}

fn image_list(value: &Value, key: &str) -> Option<Vec<CatalogImage>> {
    let images = value.get(key).filter(|value| !value.is_null())?;
    let Some(images) = images.as_array() else {
        return Some(vec![CatalogImage::default()]);
    };
    Some(
        images
            .iter()
            .map(|image| {
                let url = image["m_path"].as_str().or_else(|| image["l_path"].as_str()).unwrap_or_default();
                let url = if url.starts_with("//") { format!("https:{url}") } else { url.to_owned() };
                let valid = Url::parse(&url).is_ok_and(|parsed| {
                    matches!(parsed.scheme(), "https" | "http")
                        && parsed.host_str().is_some()
                        && parsed.username().is_empty()
                        && parsed.password().is_none()
                        && url.len() <= 2048
                });
                CatalogImage { url: if valid { url } else { String::new() }, ..CatalogImage::default() }
            })
            .collect(),
    )
}

// 目录展示用的是供应商市场价，不是采购结算价。缺失留空、零值照录，负数或格式错误由采集按单项失败处理。
fn market_price(spec: &Value) -> ConnectorResult<Option<Amount>> {
    if spec["market_price"].is_null() {
        return Ok(None);
    }
    let price: Amount =
        fixed(&spec["market_price"]).map_err(|e| context(e, "供应商提供的规格市场价不是有效金额"))?;
    if price.to_decimal().is_sign_negative() {
        return Err(context(mapping(), "供应商提供的规格市场价不能为负数"));
    }
    Ok(Some(price))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn optional_spec_barcode_preserves_zeroes_and_rejects_numeric_codes() {
        assert_eq!(
            details(&json!({}), &json!({"barcode":"0012345678905"}), None).unwrap().barcode.as_deref(),
            Some("0012345678905")
        );
        assert_eq!(details(&json!({}), &json!({}), None).unwrap().barcode, None);
        assert!(details(&json!({}), &json!({"barcode":123456}), None).is_err());
    }

    #[test]
    fn official_detail_fields_preserve_order_missing_and_explicit_clear() {
        let product = json!({"brand_id":"721-101902","brand_name":"品牌","cat_id":"1","cat_id2":"12","cat_id3":"123","description":" 介绍 "});
        let root = json!({"product":{"status":"1","can_buy":"1"},"brand":{"status":"1"},"images":[{"id":"1","m_path":"https://img.dangaoss.com/one.jpg"},{"id":"2","m_path":"https://img.dangaoss.com/two.jpg"}],"content_images":[]});
        let spec = json!({"deleted":"0","is_up":"1","can_buy":"1","stock":"2"});
        let value = details(&product, &spec, Some(&root)).unwrap();
        assert_eq!(value.category_id.as_deref(), Some("123"));
        assert_eq!(value.brand_id.as_deref(), Some("721-101902"));
        assert_eq!(value.description.as_deref(), Some("介绍"));
        assert_eq!(value.images.as_ref().unwrap()[1].url, "https://img.dangaoss.com/two.jpg");
        assert!(value.images.unwrap().iter().all(|image| image.content_fingerprint.is_none()));
        assert_eq!(value.detail_images, Some(vec![]));
        assert_eq!(value.availability, CatalogAvailability::Available);
        assert_eq!(value.available_quantity.as_deref(), Some("2"));
        assert_eq!(details(&product, &spec, None).unwrap().images, None);
    }

    #[test]
    fn catalog_availability_requires_can_buy_but_ignores_is_up() {
        let root = json!({"product":{"status":"1","can_buy":"1"},"brand":{"status":"1"}});
        for is_up in [None, Some(json!("0")), Some(json!("1")), Some(json!("invalid"))] {
            let mut spec = json!({"deleted":"0","can_buy":"1","stock":"-9999999"});
            if let Some(is_up) = is_up {
                spec["is_up"] = is_up;
            }
            assert_eq!(
                details(&root["product"], &spec, Some(&root)).unwrap().availability,
                CatalogAvailability::Available
            );
            spec["can_buy"] = json!("0");
            assert_eq!(
                details(&root["product"], &spec, Some(&root)).unwrap().availability,
                CatalogAvailability::Unavailable
            );
            spec["can_buy"] = Value::Null;
            assert_eq!(
                details(&root["product"], &spec, Some(&root)).unwrap().availability,
                CatalogAvailability::Unknown
            );
            spec["can_buy"] = json!("invalid");
            assert!(details(&root["product"], &spec, Some(&root)).is_err());
        }
    }

    #[test]
    fn catalog_stock_preserves_error_precedence_and_incomplete_state_validation() {
        let mut root = json!({"product":{"status":"1", "can_buy":"1"},
            "brand":{"status":"invalid"}});
        let mut spec = json!({"deleted":"0", "can_buy":"1", "stock":"-1"});
        let stock_error = context(mapping(), "供应商提供的规格库存无效");
        assert_eq!(details(&json!({}), &spec, Some(&root)).unwrap_err(), stock_error);
        spec["stock"] = json!("2.500000");
        assert_eq!(
            details(&json!({}), &spec, Some(&root)).unwrap_err(),
            context(mapping(), "供应商品牌的启用状态无法识别")
        );

        root["product"]["can_buy"] = Value::Null;
        let value = details(&json!({}), &spec, Some(&root)).unwrap();
        assert_eq!(value.availability, CatalogAvailability::Unknown);
        assert_eq!(value.available_quantity.as_deref(), Some("2.500000"));
        spec["stock"] = json!("-1");
        assert_eq!(details(&json!({}), &spec, Some(&root)).unwrap_err(), stock_error);

        let listed = details(&json!({}), &spec, None).unwrap();
        assert_eq!(listed.availability, CatalogAvailability::Unknown);
        assert_eq!(listed.available_quantity, None);
    }

    #[test]
    fn image_lists_preserve_absent_clear_and_invalid_placeholders() {
        for root in [json!({}), json!({"images":null, "content_images":null})] {
            let value = details(&json!({}), &json!({}), Some(&root)).unwrap();
            assert_eq!(value.images, None);
            assert_eq!(value.detail_images, None);
        }

        let root = json!({"images":[], "content_images":[]});
        let value = details(&json!({}), &json!({}), Some(&root)).unwrap();
        assert_eq!(value.images, Some(vec![]));
        assert_eq!(value.detail_images, Some(vec![]));

        for malformed in [json!("invalid"), json!({}), json!(true), json!(1)] {
            let root = json!({"images":malformed, "content_images":malformed});
            let value = details(&json!({}), &json!({}), Some(&root)).unwrap();
            let placeholder = CatalogImage::default();
            assert_eq!(value.images, Some(vec![placeholder.clone()]));
            assert_eq!(value.detail_images, Some(vec![placeholder]));
        }
    }

    #[test]
    fn invalid_media_and_unreported_inventory_never_become_valid_facts() {
        assert_eq!(
            image_list(&json!({"images":[{"m_path":"file:///secret"}]}), "images").unwrap()[0].url,
            ""
        );

        let root = json!({"product":{"status":"1","can_buy":"1"},"brand":{"status":"1"}});
        let spec = json!({"deleted":"0","is_up":"1","can_buy":"1","stock":"-9999999"});
        let value = details(&json!({}), &spec, Some(&root)).unwrap();
        assert_eq!(value.available_quantity, None);
        assert!(!value.quantity_unbounded);
    }

    #[test]
    fn protocol_relative_images_use_https_and_invalid_addresses_report_position() {
        let images = image_list(&json!({"images":[{"m_path":"//img31.hua.com/a.jpg"}]}), "images").unwrap();
        assert_eq!(images[0].url, "https://img31.hua.com/a.jpg");
        for url in ["file:///secret", "//user:pass@example.com/a", "//", "https://user@example.com/a"] {
            let image = image_list(&json!({"images":[{"m_path":url}]}), "images").unwrap();
            assert_eq!(image[0].url, "");
        }
    }
    #[test]
    fn source_market_price_is_optional_exact_and_independent_of_other_prices() {
        assert_eq!(market_price(&json!({"price":"99", "clearing_price":"80"})).unwrap(), None);
        for value in ["0", "129.50"] {
            assert_eq!(market_price(&json!({"market_price":value})).unwrap(), Some(value.parse().unwrap()));
        }
        for value in ["-1", "1.001", "invalid", ""] {
            assert!(market_price(&json!({"market_price":value})).is_err());
        }
    }
}
