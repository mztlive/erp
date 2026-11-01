//! 补图时从当前修订构造正式更新输入，不重放种子价格或规格。

use erp_catalog::entity::catalog::product_revision_media::{MediaRole, ProductRevisionMedia};
use erp_catalog::entity::catalog::{
    Product, ProductRevision, Sku, SkuRevision, parse_specification_signature,
};
use erp_catalog::{ProductMediaInput, ProductSkuInput, SpecEntryInput, UpdateProductRequest};
use erp_core::ids::{SkuId, SkuRevisionId};

use crate::Result;

/// 保留当前商品字段及全部现有媒体。
/// # 参数
/// 当前商品、修订、媒体及全部启用 SKU 的更新输入。
/// # 返回
/// 仅供补图流程使用的正式商品更新请求。
pub(super) fn product_request(
    product: &Product,
    revision: &ProductRevision,
    media: &[ProductRevisionMedia],
    skus: Vec<ProductSkuInput>,
) -> UpdateProductRequest {
    UpdateProductRequest {
        version: product.base.version,
        change_reason: Some("补齐演示商品图片".into()),
        name: revision.name.clone(),
        description: revision.description.clone(),
        specification: revision.specification.clone(),
        category_id: revision.category_id.clone(),
        brand_id: revision.brand_id.clone(),
        status: revision.status,
        effective_from: revision.effective_from,
        effective_to: revision.effective_to,
        carousel_media: media_inputs(media, MediaRole::Carousel),
        detail_media: media_inputs(media, MediaRole::Detail),
        skus,
    }
}

fn media_inputs(media: &[ProductRevisionMedia], role: MediaRole) -> Vec<ProductMediaInput> {
    media
        .iter()
        .filter(|item| item.media_role == role)
        .map(|item| ProductMediaInput {
            file_asset_id: item.file_asset_id.clone(),
            sort_order: item.sort_order,
            alt_text: item.alt_text.clone(),
        })
        .collect()
}

/// 保留稳定身份、精确修订、三价和规格。
/// # 参数
/// 当前 SKU 稳定实体及其当前修订。
/// # 返回
/// 保留原值的 SKU 更新输入。
/// # 错误
/// 历史规格签名不合法时拒绝更新，禁止改成空规格。
pub(super) fn sku_input(sku: &Sku, revision: &SkuRevision) -> Result<ProductSkuInput> {
    let spec_entries = parse_specification_signature(&sku.specification_signature)?
        .into_iter()
        .map(|entry| SpecEntryInput {
            attribute_code: entry.attribute_code,
            attribute_value_code: entry.value_code,
        })
        .collect();
    Ok(ProductSkuInput {
        sku_id: Some(SkuId::new(sku.base.id.clone())),
        expected_sku_revision_id: Some(SkuRevisionId::new(revision.base.id.clone())),
        reenable: false,
        sku_no: sku.sku_no.clone(),
        name: revision.name.clone(),
        base_unit_id: sku.base_unit_id.clone(),
        barcode: revision.barcode.clone(),
        main_image_asset_id: revision.source_main_image_asset_id.clone(),
        weight_kg: revision.weight_kg,
        volume_m3: revision.volume_m3,
        sales_visible_price_gross: revision.sales_visible_price_gross,
        bulk_price_gross: revision.bulk_price_gross,
        bulk_min_quantity: revision.bulk_min_quantity,
        market_price: revision.market_price,
        spec_entries,
    })
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_catalog::entity::catalog::{EnableStatus, ListingStatus};
    use erp_core::common::revision::RevisionBase;
    use erp_core::common::stable::StableBase;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{FileAssetId, ProductId, UnitOfMeasureId};

    use super::*;

    #[test]
    fn image_update_preserves_current_sku_prices_identity_and_manual_image() {
        let mut sku = Sku {
            base: BaseModel::fake(),
            stable: StableBase::new(EnableStatus::Active, "admin"),
            sku_no: "DEMO-MD-SKU-01".into(),
            product_id: ProductId::new("p"),
            base_unit_id: UnitOfMeasureId::new("box"),
            specification_signature: "颜色=蓝色".into(),
            listing_status: ListingStatus::Listed,
        };
        sku.base.id = "sku".into();
        let revision = SkuRevision {
            base: BaseModel::fake(),
            revision: RevisionBase { revision_no: 7 },
            sku_id: SkuId::new("sku"),
            name: "人工调整后的品名".into(),
            description: None,
            specification: None,
            barcode: Some("123".into()),
            source_main_image_asset_id: Some(FileAssetId::new("manual-image")),
            weight_kg: Some("1.25".parse().unwrap()),
            volume_m3: None,
            sales_visible_price_gross: Some("129".parse().unwrap()),
            bulk_price_gross: Some("119".parse().unwrap()),
            bulk_min_quantity: Some("10".parse().unwrap()),
            market_price: Some("159".parse().unwrap()),
            status: EnableStatus::Active,
            effective_from: BusinessDate::today(),
            effective_to: None,
        };
        let input = sku_input(&sku, &revision).unwrap();
        assert_eq!(input.sku_id.as_ref().map(AsRef::as_ref), Some("sku"));
        assert_eq!(
            input.expected_sku_revision_id.as_ref().map(AsRef::as_ref),
            Some(revision.base.id.as_str())
        );
        assert_eq!(input.name, revision.name);
        assert_eq!(input.main_image_asset_id, revision.source_main_image_asset_id);
        assert_eq!(
            (
                input.sales_visible_price_gross,
                input.bulk_price_gross,
                input.bulk_min_quantity,
                input.market_price
            ),
            (
                revision.sales_visible_price_gross,
                revision.bulk_price_gross,
                revision.bulk_min_quantity,
                revision.market_price
            )
        );
        assert_eq!(input.spec_entries[0].attribute_value_code, "蓝色");
        sku.specification_signature = "broken|signature".into();
        assert!(sku_input(&sku, &revision).is_err());
    }
}
