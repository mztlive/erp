//! 给已导入但缺图的商品补齐轮播图与 SKU 主图。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_catalog::entity::catalog::product_revision_media::{MediaRole, ProductRevisionMedia};
use erp_catalog::entity::catalog::{
    EnableStatus, Product, ProductRevision, Sku, SkuRevision, parse_specification_signature,
};
use erp_catalog::repository::prelude::*;
use erp_catalog::{CatalogExt, ProductMediaInput, ProductSkuInput, SpecEntryInput, UpdateProductRequest};
use erp_core::ids::{ProductId, SkuId, SkuRevisionId};
use erp_master_data::product_update_with_assets;
use erp_support::PendingFileAssetRequest;
use persistence_core::ReadExecutor;

use super::ProductImportProcess;
use super::identity::NormalizedImportRow;
use super::images::{RowMedia, RowMediaSource, resolve_row_media};
use super::row::RowImportOutcome;
use crate::{Error, Result};

impl ProductImportProcess {
    // 历史任务按导入行定位已有 SKU，使用同一补图规则。
    pub(super) async fn import_existing_with_images(
        &self,
        product: Product,
        row: &NormalizedImportRow,
        media_source: &RowMediaSource<'_>,
        actor: &AuditActor,
    ) -> Result<RowImportOutcome> {
        let skus = self
            .db
            .skus()
            .find_by_product_ids(&[ProductId::new(product.base.id.clone())], &mut ReadExecutor::default())
            .await?;
        let revisions = self.db.catalog().current_sku_revisions(&skus, &mut ReadExecutor::default()).await?;
        let Some(sku) = row.matching_sku(&skus, &revisions)?.filter(|sku| can_backfill(sku, &revisions))
        else {
            return Ok(already_imported(product.base.id));
        };
        let media = resolve_row_media(&self.storage, &self.secret, media_source).await?;
        let Some((request, pending)) =
            self.prepare_image_backfill(&product, sku, &skus, &revisions, media).await?
        else {
            return Ok(already_imported(product.base.id));
        };
        match product_update_with_assets(
            self.services.scoped_catalog.clone(),
            product.base.id.clone(),
            request,
            pending,
            actor.clone(),
        )
        .await
        .map_err(Error::from)
        {
            Ok(_) => {
                Ok(RowImportOutcome::new("商品已存在，已补齐图片".into())
                    .with_product_id(Some(product.base.id)))
            },
            Err(Error::ConflictError(_)) => Ok(already_imported(product.base.id)),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn prepare_image_backfill(
        &self,
        product: &Product,
        sku: &Sku,
        skus: &[Sku],
        revisions: &HashMap<String, SkuRevision>,
        media: RowMedia,
    ) -> Result<Option<(UpdateProductRequest, Vec<PendingFileAssetRequest>)>> {
        if media.pending.is_empty() || !can_backfill(sku, revisions) {
            return Ok(None);
        }
        let snapshot = self
            .db
            .catalog()
            .product_disable_snapshot(&product.base.id, &mut ReadExecutor::default())
            .await?
            .ok_or_else(|| Error::NotFound("商品不存在".into()))?;
        let revision = snapshot
            .current_revision
            .ok_or_else(|| Error::BusinessLogicError("商品缺少当前资料，无法补图".into()))?;
        backfill_request(product, revision, &snapshot.media, sku, skus, revisions, media)
    }
}

fn can_backfill(sku: &Sku, revisions: &HashMap<String, SkuRevision>) -> bool {
    sku.stable.status == EnableStatus::Active
        && revisions.get(&sku.base.id).is_some_and(|revision| revision.source_main_image_asset_id.is_none())
}

// 只补匹配规格的空白主图。已有轮播和详情图的文件、顺序和说明随修订保留。
fn backfill_request(
    product: &Product,
    revision: ProductRevision,
    existing_media: &[ProductRevisionMedia],
    sku: &Sku,
    skus: &[Sku],
    revisions: &HashMap<String, SkuRevision>,
    mut media: RowMedia,
) -> Result<Option<(UpdateProductRequest, Vec<PendingFileAssetRequest>)>> {
    let main_image = media.main_image.take().filter(|_| {
        revisions.get(&sku.base.id).is_some_and(|revision| revision.source_main_image_asset_id.is_none())
    });
    let add_carousel = existing_media.is_empty()
        && !media.carousel.is_empty()
        && !revisions.values().any(|revision| revision.source_main_image_asset_id.is_some());
    if main_image.is_none() && !add_carousel {
        return Ok(None);
    }

    let mut sku_inputs = existing_sku_inputs(skus, revisions)?;
    if let Some(image) = main_image
        && let Some(input) = sku_inputs
            .iter_mut()
            .find(|input| input.sku_id.as_ref().is_some_and(|id| id.as_ref() == sku.base.id))
    {
        input.main_image_asset_id = Some(image);
    }
    let carousel_media = if add_carousel {
        media
            .carousel
            .into_iter()
            .map(|(id, sort_order)| ProductMediaInput { file_asset_id: id, sort_order, alt_text: None })
            .collect()
    } else {
        media_inputs(existing_media, MediaRole::Carousel)
    };

    // 目录命令只接受实际引用的待登记附件。
    media.pending.retain(|entry| {
        carousel_media.iter().any(|item| item.file_asset_id.as_ref() == entry.reference)
            || sku_inputs
                .iter()
                .any(|sku| sku.main_image_asset_id.as_ref().is_some_and(|id| id.as_ref() == entry.reference))
    });
    let request = UpdateProductRequest {
        version: product.base.version,
        change_reason: Some("产品报价表导入补齐图片".into()),
        name: revision.name,
        description: revision.description,
        specification: revision.specification,
        category_id: revision.category_id,
        brand_id: revision.brand_id,
        status: product.stable.status,
        effective_from: revision.effective_from,
        effective_to: revision.effective_to,
        carousel_media,
        detail_media: media_inputs(existing_media, MediaRole::Detail),
        skus: sku_inputs,
    };
    Ok(Some((request, media.pending)))
}

// 按媒体用途复制当前修订的文件身份、展示顺序和替代文本。
pub(super) fn media_inputs(media: &[ProductRevisionMedia], role: MediaRole) -> Vec<ProductMediaInput> {
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

fn already_imported(product_id: String) -> RowImportOutcome {
    RowImportOutcome::new("商品已存在，本行未重复写入".into())
        .with_product_id(Some(product_id))
        .with_skipped(true)
}

// 把当前启用 SKU 转成编辑请求中的保留行。
//
// `skus`：商品下 SKU；`revisions`：当前修订。
//
// 返回可提交的 SKU 行。
//
// 规格签名无法解析或没有启用 SKU 时返回错误。
pub(super) fn existing_sku_inputs(
    skus: &[Sku],
    revisions: &HashMap<String, SkuRevision>,
) -> Result<Vec<ProductSkuInput>> {
    let mut inputs = Vec::new();
    for sku in skus.iter().filter(|sku| sku.stable.status == EnableStatus::Active) {
        let Some(revision) = revisions.get(&sku.base.id) else {
            continue;
        };
        let spec_entries = parse_specification_signature(&sku.specification_signature)
            .map_err(|_| {
                Error::BusinessLogicError("已有商品的规格资料无法读取，请先在商品档案中核对规格".into())
            })?
            .into_iter()
            .map(|entry| SpecEntryInput {
                attribute_code: entry.attribute_code,
                attribute_value_code: entry.value_code,
            })
            .collect();
        inputs.push(ProductSkuInput {
            sku_id: Some(SkuId::new(sku.base.id.clone())),
            expected_sku_revision_id: Some(SkuRevisionId::new(
                sku.stable.current_revision_id.clone().unwrap_or_else(|| revision.base.id.clone()),
            )),
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
        });
    }

    if inputs.is_empty() {
        return Err(Error::BusinessLogicError("商品没有可补图的 SKU".into()));
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_catalog::entity::catalog::product::ProductData;
    use erp_catalog::entity::catalog::product_revision::ProductRevisionData;
    use erp_catalog::entity::catalog::{
        ListingStatus, ProductKind, SpecSignatureEntry, compute_specification_signature,
    };
    use erp_catalog::entity::sku::SkuData;
    use erp_catalog::entity::sku_revision::SkuRevisionData;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{FileAssetId, ProductBrandId, ProductCategoryId, ProductRevisionId, UnitOfMeasureId};
    use erp_support::{RegisterFileAssetRequest, RetentionClass, SensitivityClass, content_fingerprint};

    use super::*;

    #[test]
    fn reimport_fills_only_the_matched_sku_and_keeps_existing_media_and_prices() {
        let sku = Sku::new(
            SkuId::new("sku-1"),
            SkuData {
                sku_no: "SKU-1".into(),
                product_id: ProductId::new("product-1"),
                base_unit_id: UnitOfMeasureId::new("unit-1"),
                specification_signature: String::new(),
                status: EnableStatus::Active,
                listing_status: ListingStatus::Listed,
            },
            "actor-1",
        )
        .unwrap();
        let revision = SkuRevision::new(
            SkuRevisionId::new("revision-1"),
            SkuRevisionData {
                sku_id: SkuId::new("sku-1"),
                revision_no: 1,
                name: "商品".into(),
                description: None,
                specification: None,
                barcode: None,
                source_main_image_asset_id: Some(FileAssetId::new("image-1")),
                weight_kg: None,
                volume_m3: None,
                sales_visible_price_gross: Some("45.00".parse().unwrap()),
                bulk_price_gross: Some("40.00".parse().unwrap()),
                bulk_min_quantity: Some("12".parse().unwrap()),
                market_price: Some("60.00".parse().unwrap()),
                status: EnableStatus::Active,
                effective_from: "2026-10-03".parse::<BusinessDate>().unwrap(),
                effective_to: None,
            },
        )
        .unwrap();
        let mut target = sku.clone();
        target.base.id = "sku-2".into();
        target.sku_no = "SKU-2".into();
        target.specification_signature = compute_specification_signature(&[SpecSignatureEntry {
            attribute_code: "规格".into(),
            value_code: "大".into(),
        }])
        .unwrap();
        let mut target_revision = revision.clone();
        target_revision.base.id = "revision-2".into();
        target_revision.sku_id = SkuId::new("sku-2");
        target_revision.source_main_image_asset_id = None;
        let skus = [sku, target];
        let revisions =
            HashMap::from([("sku-1".into(), revision.clone()), ("sku-2".into(), target_revision)]);
        let product = Product::new(
            ProductId::new("product-1"),
            ProductData {
                product_no: "product-no".into(),
                product_kind: ProductKind::Physical,
                status: EnableStatus::Active,
                maintainer_user_id: "actor-1".into(),
                business_org_unit_id: "org-1".into(),
            },
            "actor-1",
        )
        .unwrap();
        let product_revision_id = ProductRevisionId::new("product-revision-1");
        let product_revision = ProductRevision::new(
            product_revision_id.clone(),
            ProductRevisionData {
                product_id: ProductId::new("product-1"),
                revision_no: 1,
                name: "商品".into(),
                description: None,
                specification: None,
                category_id: ProductCategoryId::new("category-1"),
                brand_id: ProductBrandId::new("brand-1"),
                status: EnableStatus::Active,
                effective_from: revision.effective_from,
                effective_to: None,
            },
        )
        .unwrap();
        let existing_media =
            [MediaRole::Carousel, MediaRole::Detail].map(|media_role| ProductRevisionMedia {
                base: BaseModel::fake(),
                product_revision_id: product_revision_id.clone(),
                file_asset_id: FileAssetId::new("existing-media"),
                media_role,
                sort_order: 2,
                alt_text: Some("原图".into()),
            });
        let pending =
            ["pending-file:main", "pending-file:carousel"].map(|reference| PendingFileAssetRequest {
                reference: reference.into(),
                registration: RegisterFileAssetRequest {
                    storage_object_key: format!("{reference}.png"),
                    file_name: "test.png".into(),
                    content_type: "image/png".into(),
                    byte_size: 10,
                    content_hmac: content_fingerprint("test", b"test"),
                    retention_class: RetentionClass::LongTerm,
                    sensitivity_class: SensitivityClass::General,
                    expires_at: None,
                },
            });
        let media = RowMedia {
            main_image: Some(FileAssetId::new("pending-file:main")),
            carousel: vec![(FileAssetId::new("pending-file:carousel"), 0)],
            pending: pending.to_vec(),
        };
        assert!(
            backfill_request(
                &product,
                product_revision.clone(),
                &existing_media,
                &skus[0],
                &skus,
                &revisions,
                media.clone()
            )
            .unwrap()
            .is_none()
        );
        let (request, pending) =
            backfill_request(&product, product_revision, &existing_media, &skus[1], &skus, &revisions, media)
                .unwrap()
                .unwrap();
        let input = &request.skus[0];
        assert_eq!(input.main_image_asset_id.as_ref().unwrap().as_ref(), "image-1");
        assert_eq!(input.sales_visible_price_gross, revision.sales_visible_price_gross);
        assert_eq!(input.bulk_price_gross, revision.bulk_price_gross);
        assert_eq!(input.bulk_min_quantity, revision.bulk_min_quantity);
        assert_eq!(input.market_price, revision.market_price);
        assert_eq!(request.skus[1].main_image_asset_id.as_ref().unwrap().as_ref(), "pending-file:main");
        assert_eq!(request.skus[1].bulk_price_gross, revision.bulk_price_gross);
        assert_eq!(request.carousel_media[0].file_asset_id.as_ref(), "existing-media");
        assert_eq!(request.carousel_media[0].sort_order, 2);
        assert_eq!(request.detail_media[0].alt_text.as_deref(), Some("原图"));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].reference, "pending-file:main");
    }
}
