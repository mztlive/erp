//! 官方新版列表与详情字段归一；不将图片地址或来源编号冒充内容指纹。
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::offer::{
    AvailabilityState, AvailableQuantity, CatalogAvailability, CatalogImage, OfferDetails,
};
use reqwest::Url;
use serde_json::Value;

use super::parsing::{context, id, mapping};

/// 提取目录资料，并在必需状态完整时计算可供性；`is_up` 不参与完整性检查。
/// # 参数
/// `product` 与 `spec` 为来源商品和规格，`root` 为可选的完整详情响应。
/// # 返回
/// 规范化资料；缺少必需状态时保留未知可供状态。
/// # 错误
/// 来源标识、素材、库存或参与判定的状态字段非法时返回映射错误。
pub(super) fn details(product: &Value, spec: &Value, root: Option<&Value>) -> ConnectorResult<OfferDetails> {
    let category_id = optional_id(product, "cat_id3")?
        .or(optional_id(product, "cat_id2")?)
        .or(optional_id(product, "cat_id")?);
    let mut details = OfferDetails {
        category_id,
        category_name: None, // 由独立分类字典补齐；label_name 是商品标签，不得冒充分类。
        brand_id: optional_id(product, "brand_id")?,
        brand_name: optional_text(product, "brand_name")?,
        description: optional_text(product, "description")?,
        ..OfferDetails::default()
    };
    if let Some(root) = root {
        details.images = image_list(root, "images")?;
        details.detail_images = image_list(root, "content_images")?;
        details.brand_name = details.brand_name.or(optional_text(&root["brand"], "name")?);
        let complete_state = !root["brand"]["status"].is_null()
            && !root["product"]["status"].is_null()
            && !root["product"]["can_buy"].is_null()
            && !spec["deleted"].is_null()
            && !spec["can_buy"].is_null();
        details.availability = if complete_state {
            match super::catalog::availability_state(root, spec, &catalog_stock(spec)?)? {
                AvailabilityState::Available => CatalogAvailability::Available,
                AvailabilityState::Unavailable { .. } => CatalogAvailability::Unavailable,
                AvailabilityState::Unknown => CatalogAvailability::Unknown,
            }
        } else {
            CatalogAvailability::Unknown
        };
        if let AvailableQuantity::Exact(quantity) = catalog_stock(spec)? {
            details.available_quantity = Some(quantity.to_string());
        }
    }
    Ok(details)
}

fn catalog_stock(spec: &Value) -> ConnectorResult<AvailableQuantity> {
    super::catalog::stock(&spec["stock"]).map_err(|e| context(e, "规格库存 stock 格式无效或为不支持的负数"))
}

fn optional_id(value: &Value, key: &str) -> ConnectorResult<Option<String>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.is_empty() || value == "0" => Ok(None),
        Some(Value::Number(value)) if value.as_u64() == Some(0) => Ok(None),
        Some(value) => id(value).map(Some).map_err(|e| context(e, &format!("字段 {key} 编号格式无效"))),
    }
}

fn optional_text(value: &Value, key: &str) -> ConnectorResult<Option<String>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.trim().to_owned())),
        _ => Err(context(mapping(), &format!("字段 {key} 必须为文本"))),
    }
}

fn image_list(value: &Value, key: &str) -> ConnectorResult<Option<Vec<CatalogImage>>> {
    let Some(images) = value.get(key).filter(|value| !value.is_null()) else { return Ok(None) };
    images
        .as_array()
        .ok_or_else(|| context(mapping(), &format!("图片字段 {key} 必须为数组")))?
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let invalid = || context(mapping(), &format!("图片字段 {key} 第 {} 项地址缺失或无效", index + 1));
            let url = image["m_path"].as_str().or_else(|| image["l_path"].as_str()).ok_or_else(invalid)?;
            let url = if url.starts_with("//") { format!("https:{url}") } else { url.to_owned() };
            let parsed = Url::parse(&url).map_err(|_| invalid())?;
            if !matches!(parsed.scheme(), "https" | "http")
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || url.len() > 2048
            {
                return Err(invalid());
            }
            Ok(CatalogImage { url, content_fingerprint: None, file_asset_id: None })
        })
        .collect::<ConnectorResult<Vec<_>>>()
        .map(Some)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
    fn invalid_media_and_unreported_inventory_never_become_valid_facts() {
        assert!(image_list(&json!({"images":[{"m_path":"file:///secret"}]}), "images").is_err());
        let root = json!({"product":{"status":"1","can_buy":"1"},"brand":{"status":"1"}});
        let spec = json!({"deleted":"0","is_up":"1","can_buy":"1","stock":"-9999999"});
        let value = details(&json!({}), &spec, Some(&root)).unwrap();
        assert_eq!(value.available_quantity, None);
        assert!(!value.quantity_unbounded);
    }

    #[test]
    fn protocol_relative_images_use_https_and_invalid_addresses_report_position() {
        let images =
            image_list(&json!({"images":[{"m_path":"//img31.hua.com/a.jpg"}]}), "images").unwrap().unwrap();
        assert_eq!(images[0].url, "https://img31.hua.com/a.jpg");
        for url in ["file:///secret", "//user:pass@example.com/a", "//", "https://user@example.com/a"] {
            let error = image_list(&json!({"images":[{"m_path":url}]}), "images").unwrap_err();
            assert_eq!(error.summary, "图片字段 images 第 1 项地址缺失或无效");
            assert!(!error.summary.contains(url));
        }
    }
}
