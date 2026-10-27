use erp_catalog::{CreateProductRequest, EnableStatus, ProductKind, ProductSkuInput, SpecEntryInput};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::money::UnitPrice;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_supply::catalog_adoption::{AdoptionRow, AdoptionTarget};
use erp_supply::dto::supplier_offering::CreateSupplierOfferingRequest;
use erp_supply::entity::supplier_catalog::{CatalogAvailability, SupplierCatalogItem};
use erp_supply::entity::supplier_offering::{AvailabilityStatus, OfferingSourceType};

use crate::source_media::PreparedSourceMedia;
use crate::{Error, Result};

pub(super) fn product_request(
    item: &SupplierCatalogItem,
    row: &AdoptionRow,
    media: &PreparedSourceMedia,
) -> Result<CreateProductRequest> {
    let AdoptionTarget::Create { category_id, brand_id, product_kind, .. } = &row.target else {
        return Err(Error::ValidationError("请选择新建商品".into()));
    };
    let kind: ProductKind = serde_json::from_value(serde_json::Value::String(product_kind.clone()))
        .map_err(|_| Error::ValidationError("商品业务类型无效".into()))?;
    if kind == ProductKind::Voucher {
        return Err(Error::ValidationError("API 商品选品不创建卡券类目".into()));
    }
    let (name, description) = company_display_texts(item, &row.target)?;
    Ok(CreateProductRequest {
        change_reason: Some("供应商 API 商品选品入库".into()),
        product_no: None,
        product_kind: kind,
        maintainer_user_id: None,
        name,
        description,
        specification: None,
        category_id: category_id.clone().into(),
        brand_id: brand_id.clone().into(),
        status: Some(EnableStatus::Active),
        effective_from: BusinessDate::today(),
        effective_to: None,
        carousel_media: media.carousel_media.clone(),
        detail_media: media.detail_media.clone(),
        skus: vec![sku_input(item, row, media)?],
    })
}

pub(super) fn sku_input(
    item: &SupplierCatalogItem,
    row: &AdoptionRow,
    media: &PreparedSourceMedia,
) -> Result<ProductSkuInput> {
    let AdoptionTarget::Create { unit_id, spec_entries, .. } = &row.target else {
        return Err(Error::ValidationError("请选择新建规格".into()));
    };
    if unit_id.trim().is_empty() || (!item.specification.trim().is_empty() && spec_entries.is_empty()) {
        return Err(Error::ValidationError("请选择单位并填写来源规格映射".into()));
    }
    Ok(ProductSkuInput {
        sku_id: None,
        expected_sku_revision_id: None,
        reenable: false,
        sku_no: String::new(),
        name: company_display_texts(item, &row.target)?.0,
        base_unit_id: unit_id.clone().into(),
        barcode: None,
        main_image_asset_id: media
            .carousel_media
            .iter()
            .find(|image| image.sort_order == 0)
            .map(|image| image.file_asset_id.clone()),
        weight_kg: None,
        volume_m3: None,
        sales_visible_price_gross: None,
        bulk_price_gross: None,
        bulk_min_quantity: None,
        market_price: None,
        spec_entries: spec_entries
            .iter()
            .map(|entry| SpecEntryInput {
                attribute_code: entry.attribute_code.clone(),
                attribute_value_code: entry.attribute_value_code.clone(),
            })
            .collect(),
    })
}

fn company_display_texts(
    item: &SupplierCatalogItem,
    target: &AdoptionTarget,
) -> Result<(String, Option<String>)> {
    let AdoptionTarget::Create { company_name, company_description, .. } = target else {
        return Err(Error::ValidationError("请选择新建公司商品".into()));
    };
    let name = normalize_required_text(
        company_name.clone().unwrap_or_else(|| item.name.clone()),
        "公司商品名称不能为空",
        128,
        "来源名称超过公司字段128字限制，请填写公司展示名称",
    )?;
    let description = normalize_optional_text(
        company_description.clone().or_else(|| item.description.clone()),
        "公司商品介绍",
        512,
    )
    .map_err(|_| {
        Error::ValidationError(
            "来源介绍超过公司字段512字限制，请整理公司介绍或明确清空；完整来源资料保留".into(),
        )
    })?;
    Ok((name, description))
}

pub(super) fn offering_request(
    item: &SupplierCatalogItem,
    row: &AdoptionRow,
    sku_id: &str,
    command: &str,
) -> Result<CreateSupplierOfferingRequest> {
    if let Some(price) = &item.purchase_price {
        let source: UnitPrice =
            price.parse().map_err(|_| Error::ValidationError("来源采购报价无效".into()))?;
        let confirmed: UnitPrice = row
            .terms
            .dropship_supply_price_gross
            .parse()
            .map_err(|_| Error::ValidationError("采购报价无效".into()))?;
        if source != confirmed {
            return Err(Error::ConflictError("接口采购报价不得手工覆盖，请按当前来源报价确认".into()));
        }
    }
    let status = match item.availability {
        CatalogAvailability::Available
            if !item.requires_identity_review
                && item.availability_valid_until.is_some_and(|until| until > Instant::now()) =>
        {
            AvailabilityStatus::Available
        },
        CatalogAvailability::Available => AvailabilityStatus::Stale,
        CatalogAvailability::Unavailable => AvailabilityStatus::Unavailable,
        CatalogAvailability::Withdrawn => {
            return Err(Error::BusinessLogicError("供应商已撤回该规格，不能选品".into()));
        },
        CatalogAvailability::Unknown => AvailabilityStatus::Stale,
    };
    Ok(CreateSupplierOfferingRequest {
        sku_id: sku_id.into(),
        supplier_id: item.supplier_id.clone(),
        supplier_product_code: Some(item.product_id.clone()),
        supplier_sku_code: item.spec_id.clone(),
        source_type: OfferingSourceType::Api,
        source_connection_id: Some(item.connection_id.clone()),
        terms: row.terms.clone(),
        delivery_template_id: row.delivery_template_id.clone(),
        availability_status: status,
        available_quantity: item.available_quantity.clone(),
        source_updated_at: item.source_updated_at.map(|at| at.unix_secs()),
        source_revision_token: Some(item.source_fingerprint.clone()),
        change_reason: "供应商 API 商品选品入库".into(),
        idempotency_key: format!("adoption-{command}"),
    })
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_supply::catalog_adoption::AdoptionSpecEntry;
    use erp_supply::dto::supplier_offering::SupplierOfferingTermsWrite;
    use erp_supply::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use erp_supply::ports::connector::offer::{Offer, OfferDetails};

    use super::*;

    fn item() -> SupplierCatalogItem {
        SupplierCatalogItem::observed(
            "connection",
            "supplier",
            "job",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(100),
                },
                value: Offer {
                    sku: SupplierSku {
                        product_id: Some("external-product".into()),
                        spec_id: "external-sku".into(),
                    },
                    name: "供应商原名".into(),
                    specification: "6寸".into(),
                    unit: Some("个".into()),
                    quote: None,
                    details: OfferDetails::default(),
                },
            },
        )
        .unwrap()
    }

    fn row() -> AdoptionRow {
        AdoptionRow {
            item_id: item().base.id,
            target: AdoptionTarget::Create {
                company_name: None,
                company_description: None,
                category_id: "category".into(),
                brand_id: "brand".into(),
                unit_id: "unit".into(),
                product_kind: "PHYSICAL".into(),
                spec_entries: vec![AdoptionSpecEntry {
                    attribute_code: "尺寸".into(),
                    attribute_value_code: "6寸".into(),
                }],
                unit_confirmed: true,
                specification_confirmed: true,
            },
            terms: SupplierOfferingTermsWrite::new(
                "80.00".into(),
                "75.00".into(),
                "0.13".into(),
                "10".into(),
                "2026-10-26".into(),
            ),
            delivery_template_id: Some("delivery".into()),
        }
    }

    #[test]
    fn new_company_product_uses_source_content_without_sales_prices_or_guessed_dictionaries() {
        let request = product_request(&item(), &row(), &PreparedSourceMedia::empty()).unwrap();
        assert_eq!(request.name, "供应商原名");
        assert_eq!(request.category_id.as_ref(), "category");
        assert_eq!(request.skus[0].base_unit_id.as_ref(), "unit");
        assert!(request.skus[0].sales_visible_price_gross.is_none());
        assert!(request.skus[0].bulk_price_gross.is_none());
        assert!(request.skus[0].market_price.is_none());
        assert_eq!(request.skus[0].spec_entries[0].attribute_value_code, "6寸");
    }

    #[test]
    fn missing_unit_or_source_specification_and_voucher_are_rejected_without_confirmation_flags() {
        let mut row = row();
        if let AdoptionTarget::Create { unit_confirmed, specification_confirmed, .. } = &mut row.target {
            *unit_confirmed = false;
            *specification_confirmed = false;
        }
        assert!(product_request(&item(), &row, &PreparedSourceMedia::empty()).is_ok());
        if let AdoptionTarget::Create { unit_id, .. } = &mut row.target {
            *unit_id = String::new();
        }
        assert!(product_request(&item(), &row, &PreparedSourceMedia::empty()).is_err());
        if let AdoptionTarget::Create { unit_id, spec_entries, .. } = &mut row.target {
            *unit_id = "unit".into();
            spec_entries.clear();
        }
        assert!(product_request(&item(), &row, &PreparedSourceMedia::empty()).is_err());
        if let AdoptionTarget::Create { product_kind, .. } = &mut row.target {
            *product_kind = "VOUCHER".into();
        }
        assert!(product_request(&item(), &row, &PreparedSourceMedia::empty()).is_err());
    }

    #[test]
    fn source_identity_price_unknown_availability_and_missing_source_time_are_preserved() {
        let mut item = item();
        item.purchase_price = Some("80.0000".into());
        let request = offering_request(&item, &row(), "company-sku", "command").unwrap();
        assert_eq!(request.supplier_sku_code, "external-sku");
        assert_eq!(request.source_type, OfferingSourceType::Api);
        assert_eq!(request.availability_status, AvailabilityStatus::Stale);
        assert!(request.available_quantity.is_none());
        assert!(request.source_updated_at.is_none());
        item.purchase_price = Some("90.00".into());
        assert!(offering_request(&item, &row(), "company-sku", "command").is_err());
        item.purchase_price = None;
        item.availability = CatalogAvailability::Withdrawn;
        assert!(offering_request(&item, &row(), "company-sku", "command").is_err());
    }

    #[test]
    fn sku_and_carousel_share_one_preallocated_asset_without_reusing_pending_reference() {
        use erp_catalog::ProductMediaInput;
        use erp_core::ids::FileAssetId;
        use erp_supply::entity::supplier_catalog::CatalogImage;

        let mut media = PreparedSourceMedia::empty();
        media.carousel_media.push(ProductMediaInput {
            file_asset_id: FileAssetId::new("asset-one"),
            sort_order: 0,
            alt_text: None,
        });
        media.images = Some(vec![CatalogImage {
            url: "https://images.example/source.png".into(),
            content_fingerprint: Some("a".repeat(64)),
            file_asset_id: Some("asset-one".into()),
        }]);
        let request = product_request(&item(), &row(), &media).unwrap();
        assert_eq!(request.carousel_media[0].file_asset_id.as_ref(), "asset-one");
        assert_eq!(request.skus[0].main_image_asset_id.as_ref().unwrap().as_ref(), "asset-one");
    }
    #[test]
    fn detail_images_keep_order_and_do_not_substitute_for_missing_sku_main_image() {
        use erp_catalog::ProductMediaInput;
        use erp_core::ids::FileAssetId;

        let mut media = PreparedSourceMedia::empty();
        media.detail_media = ["detail-two", "detail-one"]
            .into_iter()
            .enumerate()
            .map(|(index, id)| ProductMediaInput {
                file_asset_id: FileAssetId::new(id),
                sort_order: i32::try_from(index).unwrap(),
                alt_text: None,
            })
            .collect();
        let request = product_request(&item(), &row(), &media).unwrap();
        assert!(request.carousel_media.is_empty());
        assert!(request.skus[0].main_image_asset_id.is_none());
        assert_eq!(request.detail_media[0].file_asset_id.as_ref(), "detail-two");
        assert_eq!(request.detail_media[1].file_asset_id.as_ref(), "detail-one");
        assert_eq!(request.detail_media[1].sort_order, 1);
    }
    #[test]
    fn company_text_overrides_preserve_full_source_and_allow_explicit_clear() {
        let (mut item, mut row) = (item(), row());
        item.name = "源".repeat(129);
        item.description = Some("完整介绍".repeat(200));
        assert!(company_display_texts(&item, &row.target).is_err());
        let original_description = item.description.clone();
        if let AdoptionTarget::Create { company_name, company_description, .. } = &mut row.target {
            *company_name = Some("公司整理名称".into());
            *company_description = Some(String::new());
        }
        let (name, description) = company_display_texts(&item, &row.target).unwrap();
        assert_eq!(name, "公司整理名称");
        assert_eq!(description, None);
        assert_eq!(item.description, original_description);
        assert_eq!(item.name.chars().count(), 129);
    }
}
