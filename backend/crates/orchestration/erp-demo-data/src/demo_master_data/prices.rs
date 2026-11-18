//! 演示 SKU 声明完整公司参考价；已有商品仅补空值，不覆盖人工维护值。

use erp_catalog::ProductSkuInput;
use rust_decimal::Decimal;

use super::record::DemoMasterRecord;
use crate::{Error, Result};

// 在任何生成前验证公司 SKU 的三种价格及集采起订量均存在且为正数；不要用供应商成本或 MOQ 补值，失败时指出 SKU 编号。
pub(super) fn validate(sku: &ProductSkuInput) -> Result<()> {
    for (label, value) in [
        ("一件代发价", sku.sales_visible_price_gross.map(|value| value.to_decimal())),
        ("集采价", sku.bulk_price_gross.map(|value| value.to_decimal())),
        ("市场价", sku.market_price.map(|value| value.to_decimal())),
        ("集采起订量", sku.bulk_min_quantity.map(|value| value.to_decimal())),
    ] {
        if !value.is_some_and(|value| value > Decimal::ZERO) {
            return Err(Error::ValidationError(format!("演示 SKU {} 的{label}必须为正数", sku.sku_no)));
        }
    }
    Ok(())
}

// 仅对实际 ID 已登记且编号匹配模板的 SKU 补全价格及集采门槛，保留所有已有值。
// 补到至少一项才返回 `true`，供调用方跳过无变化更新。
pub(super) fn fill_missing(
    current: &mut [ProductSkuInput],
    templates: &[ProductSkuInput],
    record: &DemoMasterRecord,
) -> bool {
    let mut changed = false;
    for sku in current {
        if !sku.sku_id.as_ref().is_some_and(|id| record.related_ids.contains(&id.to_string())) {
            continue;
        }
        let Some(template) = templates.iter().find(|template| template.sku_no == sku.sku_no) else {
            continue;
        };
        changed |= fill(&mut sku.sales_visible_price_gross, template.sales_visible_price_gross);
        changed |= fill(&mut sku.bulk_price_gross, template.bulk_price_gross);
        changed |= fill(&mut sku.market_price, template.market_price);
        changed |= fill(&mut sku.bulk_min_quantity, template.bulk_min_quantity);
    }
    changed
}

fn fill<T: Copy>(current: &mut Option<T>, template: Option<T>) -> bool {
    if current.is_none() && template.is_some() {
        *current = template;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use erp_core::ids::{FileAssetId, SkuId, SkuRevisionId};
    use serde_json::{Value, json};

    use super::*;
    use crate::demo_master_data::seed::{SeedRequest, load};

    fn template() -> ProductSkuInput {
        let steps = load(include_str!("master-data.json")).unwrap();
        let SeedRequest::Product(product) =
            &steps.iter().find(|step| step.key == "DEMO-MD-P-01").unwrap().request
        else {
            panic!("商品种子")
        };
        product.skus[0].clone()
    }

    fn record() -> DemoMasterRecord {
        DemoMasterRecord {
            key: "DEMO-MD-P-01".into(),
            kind: "product".into(),
            entity_id: "product-id".into(),
            related_ids: vec!["sku-id".into()],
            label: "商品".into(),
            removed: false,
        }
    }

    #[test]
    fn legacy_sku_gets_missing_prices_without_changing_manual_fields_and_retry_is_noop() {
        let template = template();
        let mut sku = template.clone();
        sku.sku_id = Some(SkuId::new("sku-id"));
        sku.expected_sku_revision_id = Some(SkuRevisionId::new("revision-id"));
        sku.name = "人工品名".into();
        sku.main_image_asset_id = Some(FileAssetId::new("manual-image"));
        sku.sales_visible_price_gross = Some("123.45".parse().unwrap());
        sku.bulk_price_gross = None;
        sku.market_price = None;
        sku.bulk_min_quantity = None;
        let mut expected = sku.clone();
        expected.bulk_price_gross = template.bulk_price_gross;
        expected.market_price = template.market_price;
        expected.bulk_min_quantity = template.bulk_min_quantity;

        let mut current = vec![sku];
        assert!(fill_missing(&mut current, from_ref(&template), &record()));
        assert_eq!(serde_json::to_value(&current[0]).unwrap(), serde_json::to_value(expected).unwrap());

        assert!(!fill_missing(&mut current, &[template], &record()));
    }

    #[test]
    fn partial_prices_preserve_manual_values_including_zero_and_custom_threshold() {
        let template = template();
        let mut sku = template.clone();
        sku.sku_id = Some(SkuId::new("sku-id"));
        sku.bulk_price_gross = Some("0".parse().unwrap());
        sku.market_price = Some("222.22".parse().unwrap());
        sku.bulk_min_quantity = Some("250".parse().unwrap());
        sku.sales_visible_price_gross = None;
        let mut expected = sku.clone();
        expected.sales_visible_price_gross = template.sales_visible_price_gross;

        let mut current = vec![sku];
        assert!(fill_missing(&mut current, &[template], &record()));
        assert_eq!(serde_json::to_value(&current[0]).unwrap(), serde_json::to_value(expected).unwrap());
    }

    #[test]
    fn unregistered_or_manual_skus_are_not_filled() {
        let template = template();
        let mut sku = template.clone();
        sku.bulk_price_gross = None;
        let mut unregistered = sku.clone();
        unregistered.sku_id = Some(SkuId::new("unregistered"));
        let mut manual = sku.clone();
        manual.sku_id = Some(SkuId::new("sku-id"));
        manual.sku_no = "MANUAL-SKU".into();
        let mut current = vec![sku, unregistered, manual];
        let before = serde_json::to_value(&current).unwrap();
        assert!(!fill_missing(&mut current, &[template], &record()));
        assert_eq!(serde_json::to_value(&current).unwrap(), before);
    }

    #[test]
    fn seed_rejects_missing_zero_negative_and_invalid_prices_before_generation() {
        let original: Value = serde_json::from_str(include_str!("master-data.json")).unwrap();
        for field in ["sales_visible_price_gross", "bulk_price_gross", "market_price", "bulk_min_quantity"] {
            for value in [Value::Null, json!("0"), json!("-1"), json!("NaN")] {
                let mut rows = original.clone();
                let product = rows
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|row| row["data"]["product_no"] == "DEMO-MD-P-01")
                    .unwrap();
                product["data"]["skus"][0][field] = value;
                assert!(matches!(load(&rows.to_string()), Err(Error::ValidationError(_))), "{field}");
            }
        }
    }
}
