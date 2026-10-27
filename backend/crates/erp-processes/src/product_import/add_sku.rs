//! 同一产品编码追加 SKU 到已有 SPU。

use application_core::AuditActor;
use erp_catalog::entity::catalog::product_revision_media::{MediaRole, ProductRevisionMedia};
use erp_catalog::entity::catalog::{
    Product, ProductRevision, Sku, SpecSignatureEntry, compute_specification_signature,
};
use erp_catalog::repository::prelude::*;
use erp_catalog::{CatalogExt, ProductMediaInput, ProductSkuInput, UpdateProductRequest};
use erp_core::ids::{FileAssetId, ProductId};
use persistence_core::NoTransaction;

use super::ProductImportProcess;
use super::backfill::sku_inputs_with_main_image;
use super::identity::NormalizedImportRow;
use super::images::{RowMedia, RowMediaSource, resolve_row_media};
use super::row::RowImportOutcome;
use crate::{Error, Result};

impl ProductImportProcess {
    /// 把当前行作为新 SKU 并入已有商品；规格已存在则只补图。
    ///
    /// # 参数
    /// * `product` - 已有 SPU
    /// * `row` - 规范化后的导入行
    /// * `cells` - 原单元格
    /// * `media_source` - 行媒体来源（清单复用或源文件提取）
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 新增 SKU、补图或跳过时的行结果。
    ///
    /// # 错误
    /// 规格冲突、条码占用或更新失败时返回错误。
    pub(super) async fn import_sku_into_spu(
        &self,
        product: Product,
        row: NormalizedImportRow,
        cells: &[String],
        media_source: &RowMediaSource<'_>,
        actor: &AuditActor,
    ) -> Result<RowImportOutcome> {
        let skus = self
            .db
            .skus()
            .find_by_product_ids(&[ProductId::new(product.base.id.clone())], &mut NoTransaction)
            .await?;
        let signature = sku_signature(&row)?;
        let revisions = self.db.catalog().current_sku_revisions(&skus, &mut NoTransaction).await?;
        let same_sku = skus.iter().any(|sku| sku.specification_signature == signature)
            || row.barcode.as_ref().is_some_and(|barcode| {
                revisions.values().any(|revision| revision.barcode.as_ref() == Some(barcode))
            });
        if same_sku {
            return self.import_existing_with_images(product, cells, media_source, actor).await;
        }
        if let Some(barcode) = &row.barcode {
            let owners = self.db.catalog().barcode_owner_sku_ids(barcode, &mut NoTransaction).await?;
            if !owners.is_empty() {
                return Ok(RowImportOutcome::new(format!("条码 {barcode} 已被其他 SKU 使用，本行未写入"))
                    .with_product_id(Some(product.base.id))
                    .with_skipped(true));
            }
        }
        let media = resolve_row_media(&self.storage, &self.secret, cells, media_source).await?;
        let pending = media.pending.clone();
        let request = self.append_sku_request(&product, &row, &skus, media).await?;
        match crate::product_update_with_assets(
            self.db.clone(),
            self.rbac.clone(),
            product.base.id.clone(),
            request,
            pending,
            actor.clone(),
        )
        .await
        {
            Ok(_) => Ok(RowImportOutcome::new("已并入同一商品编码并新增 SKU".into())
                .with_product_id(Some(product.base.id))),
            Err(Error::ConflictError(_)) => {
                Ok(RowImportOutcome::new("商品正在被他人修改，本行未写入".into())
                    .with_product_id(Some(product.base.id))
                    .with_skipped(true))
            },
            Err(error) => Err(error),
        }
    }

    /// 读取现有商品修订后追加导入 SKU，保留现存 SKU 价格与媒体顺序。
    pub(super) async fn append_sku_request(
        &self,
        product: &Product,
        row: &NormalizedImportRow,
        skus: &[Sku],
        media: RowMedia,
    ) -> Result<UpdateProductRequest> {
        let snapshot = self
            .db
            .catalog()
            .product_disable_snapshot(&product.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品不存在".into()))?;
        let revision = snapshot
            .current_revision
            .ok_or_else(|| Error::BusinessLogicError("商品缺少当前资料，无法新增 SKU".into()))?;
        let revisions = self.db.catalog().current_sku_revisions(skus, &mut NoTransaction).await?;
        let sku_inputs = sku_inputs_with_main_image(skus, &revisions, None)?;
        append_request(product, row, revision, &snapshot.media, sku_inputs, media)
    }
}

/// 将当前修订和导入行组装为完整更新请求，保留既有轮播图、详情图和 SKU 资料。
fn append_request(
    product: &Product,
    row: &NormalizedImportRow,
    revision: ProductRevision,
    existing_media: &[ProductRevisionMedia],
    mut sku_inputs: Vec<ProductSkuInput>,
    media: RowMedia,
) -> Result<UpdateProductRequest> {
    sku_inputs.push(appended_sku_input(row, &sku_inputs, media.main_image)?);
    let mut carousel_media = media_inputs(existing_media, MediaRole::Carousel);
    let detail_media = media_inputs(existing_media, MediaRole::Detail);
    let mut next_order =
        carousel_media.iter().map(|item| item.sort_order).max().unwrap_or(-1).saturating_add(1);
    for (id, _) in media.carousel {
        carousel_media.push(ProductMediaInput {
            file_asset_id: id,
            sort_order: next_order,
            alt_text: Some(row.name.clone()),
        });
        next_order = next_order.saturating_add(1);
    }
    Ok(UpdateProductRequest {
        version: product.base.version,
        change_reason: Some(format!("产品报价表导入新增 SKU 第{}行", row.row_number)),
        name: revision.name,
        description: revision.description,
        specification: revision.specification,
        category_id: revision.category_id,
        brand_id: revision.brand_id,
        status: product.stable.status,
        effective_from: revision.effective_from,
        effective_to: revision.effective_to,
        carousel_media,
        detail_media,
        skus: sku_inputs,
    })
}

/// 按媒体用途复制当前修订的文件身份、展示顺序和替代文本。
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

/// 计算本行 SKU 规格签名，用于判断是否已在同一 SPU 下存在。
///
/// # 参数
/// * `row` - 规范化导入行
///
/// # 返回
/// 返回规格签名。
///
/// # 错误
/// 规格名或值非法时返回错误。
pub(super) fn sku_signature(row: &NormalizedImportRow) -> Result<String> {
    let entries = row
        .spec_entries
        .iter()
        .map(|entry| SpecSignatureEntry {
            attribute_code: entry.attribute_code.clone(),
            value_code: entry.attribute_value_code.clone(),
        })
        .collect::<Vec<_>>();
    compute_specification_signature(&entries).map_err(|error| Error::BusinessLogicError(error.to_string()))
}

/// 继承公司计量单位并维护独立三价；新编号留空，由商品服务按全部历史 SKU 统一取号。
fn appended_sku_input(
    row: &NormalizedImportRow,
    sku_inputs: &[ProductSkuInput],
    main_image_asset_id: Option<FileAssetId>,
) -> Result<ProductSkuInput> {
    Ok(ProductSkuInput {
        sku_id: None,
        expected_sku_revision_id: None,
        reenable: false,
        sku_no: String::new(),
        name: row.name.clone(),
        base_unit_id: sku_inputs
            .first()
            .map(|sku| sku.base_unit_id.clone())
            .ok_or_else(|| Error::BusinessLogicError("商品没有可继承的计量单位".into()))?,
        barcode: row.barcode.clone(),
        main_image_asset_id,
        weight_kg: None,
        volume_m3: None,
        sales_visible_price_gross: row.sales_price,
        bulk_price_gross: row.bulk_price,
        bulk_min_quantity: row.bulk_min_quantity,
        market_price: row.market_price,
        spec_entries: row.spec_entries.clone(),
    })
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_catalog::entity::catalog::product::ProductData;
    use erp_catalog::entity::catalog::product_revision::ProductRevisionData;
    use erp_catalog::entity::catalog::{EnableStatus, ProductKind};
    use erp_core::ids::{
        ProductBrandId, ProductCategoryId, ProductRevisionId, SkuId, SkuRevisionId, UnitOfMeasureId,
    };

    use super::*;
    use crate::product_import::identity::normalize_import_row;
    use crate::product_import::legacy::COMPACT_HEADERS as PRODUCT_IMPORT_HEADERS;

    /// 构造带既有 SKU 身份和价格的追加请求输入。
    fn fixture() -> (Product, NormalizedImportRow, ProductRevision, ProductSkuInput) {
        let product = Product::new(
            ProductId::new("product-1"),
            ProductData {
                product_no: "SPU20261026-000001".into(),
                product_kind: ProductKind::Physical,
                status: EnableStatus::Active,
                maintainer_user_id: "owner-1".into(),
                business_org_unit_id: "org-1".into(),
            },
            "actor-1",
        )
        .unwrap();
        let revision = ProductRevision::new(
            ProductRevisionId::new("product-revision-1"),
            ProductRevisionData {
                product_id: ProductId::new(product.base.id.clone()),
                revision_no: 1,
                name: "既有商品名称".into(),
                description: Some("既有描述".into()),
                specification: Some("既有规格说明".into()),
                category_id: ProductCategoryId::new("category-1"),
                brand_id: ProductBrandId::new("brand-1"),
                status: EnableStatus::Active,
                effective_from: "2026-10-01".parse().unwrap(),
                effective_to: Some("2027-10-01".parse().unwrap()),
            },
        )
        .unwrap();
        let existing = ProductSkuInput {
            sku_id: Some(SkuId::new("sku-2")),
            expected_sku_revision_id: Some(SkuRevisionId::new("sku-revision-2")),
            reenable: false,
            sku_no: format!("{}-02", product.product_no),
            name: "原规格".into(),
            base_unit_id: UnitOfMeasureId::new("unit-box"),
            barcode: Some("barcode-2".into()),
            main_image_asset_id: Some(FileAssetId::new("sku-image-2")),
            weight_kg: Some("2.5".parse().unwrap()),
            volume_m3: Some("0.03".parse().unwrap()),
            sales_visible_price_gross: Some("45.00".parse().unwrap()),
            bulk_price_gross: Some("40.00".parse().unwrap()),
            bulk_min_quantity: Some("12".parse().unwrap()),
            market_price: Some("60.00".parse().unwrap()),
            spec_entries: vec![],
        };
        let mut cells = vec![String::new(); PRODUCT_IMPORT_HEADERS.len()];
        cells[0] = product.product_no.clone();
        cells[6] = "导入分类".into();
        cells[7] = "新增规格名称".into();
        cells[8] = "10kg".into();
        cells[9] = "50.00".into();
        cells[10] = "48.00".into();
        cells[11] = "10".into();
        cells[12] = "70.00".into();
        (product, normalize_import_row(3, &cells).unwrap(), revision, existing)
    }

    /// 构造媒体快照，使用非连续顺序验证复制时不重新编号。
    fn stored_media(
        id: &str,
        role: MediaRole,
        sort_order: i32,
        alt_text: Option<&str>,
    ) -> ProductRevisionMedia {
        ProductRevisionMedia {
            base: BaseModel::fake(),
            product_revision_id: ProductRevisionId::new("product-revision-1"),
            file_asset_id: FileAssetId::new(id),
            media_role: role,
            sort_order,
            alt_text: alt_text.map(str::to_string),
        }
    }

    /// 检查完整更新请求的媒体字段，而非只检查中间过滤结果。
    fn media_values(media: &[ProductMediaInput]) -> Vec<(&str, i32, Option<&str>)> {
        media
            .iter()
            .map(|item| (item.file_asset_id.as_ref(), item.sort_order, item.alt_text.as_deref()))
            .collect()
    }

    /// 追加带图规格必须保留已有详情图、轮播图元数据及全部既有 SKU 输入。
    #[test]
    fn append_preserves_details_and_existing_sku_while_extending_carousel() {
        let (product, row, revision, existing) = fixture();
        let media = vec![
            stored_media("carousel-1", MediaRole::Carousel, 2, Some("轮播说明")),
            stored_media("detail-1", MediaRole::Detail, 1, Some("详情说明")),
            stored_media("carousel-2", MediaRole::Carousel, 5, None),
            stored_media("detail-2", MediaRole::Detail, 9, None),
        ];
        let uploaded = RowMedia {
            main_image: Some(FileAssetId::new("new-main")),
            carousel: vec![(FileAssetId::new("new-carousel-1"), 0), (FileAssetId::new("new-carousel-2"), 1)],
            pending: Vec::new(),
        };
        let request =
            append_request(&product, &row, revision.clone(), &media, vec![existing.clone()], uploaded)
                .unwrap();
        assert_eq!(
            media_values(&request.detail_media),
            vec![("detail-1", 1, Some("详情说明")), ("detail-2", 9, None)]
        );
        assert_eq!(
            media_values(&request.carousel_media),
            vec![
                ("carousel-1", 2, Some("轮播说明")),
                ("carousel-2", 5, None),
                ("new-carousel-1", 6, Some("新增规格名称")),
                ("new-carousel-2", 7, Some("新增规格名称")),
            ]
        );
        assert_eq!(request.version, product.base.version);
        assert_eq!(request.name, revision.name);
        assert_eq!(request.description, revision.description);
        assert_eq!(request.specification, revision.specification);
        assert_eq!(request.category_id, revision.category_id);
        assert_eq!(request.brand_id, revision.brand_id);
        assert_eq!(request.effective_from, revision.effective_from);
        assert_eq!(request.effective_to, revision.effective_to);
        assert_eq!(request.skus.len(), 2);
        assert_eq!(serde_json::to_value(&request.skus[0]).unwrap(), serde_json::to_value(&existing).unwrap());
        let appended = &request.skus[1];
        assert_eq!(appended.base_unit_id, existing.base_unit_id);
        assert_eq!(appended.main_image_asset_id, Some(FileAssetId::new("new-main")));
        assert_eq!(appended.sales_visible_price_gross, row.sales_price);
        assert_eq!(appended.bulk_price_gross, row.bulk_price);
        assert_eq!(appended.bulk_min_quantity, row.bulk_min_quantity);
        assert_eq!(appended.market_price, row.market_price);
        assert!(appended.sku_id.is_none());
        assert!(appended.expected_sku_revision_id.is_none());
        assert!(!appended.reenable);
    }

    /// 无新图片时仍保留详情图；空媒体允许新增轮播从零开始。
    #[test]
    fn append_handles_detail_only_and_empty_media() {
        let (product, row, revision, existing) = fixture();
        let detail = vec![stored_media("detail-1", MediaRole::Detail, 3, None)];
        let request = append_request(
            &product,
            &row,
            revision.clone(),
            &detail,
            vec![existing.clone()],
            RowMedia::default(),
        )
        .unwrap();
        assert_eq!(media_values(&request.detail_media), vec![("detail-1", 3, None)]);
        assert!(request.carousel_media.is_empty());
        let request = append_request(
            &product,
            &row,
            revision,
            &[],
            vec![existing],
            RowMedia { carousel: vec![(FileAssetId::new("new-carousel"), 0)], ..Default::default() },
        )
        .unwrap();
        assert!(request.detail_media.is_empty());
        assert_eq!(media_values(&request.carousel_media), vec![("new-carousel", 0, Some("新增规格名称"))]);
    }

    /// 导入请求交给正式取号方法后，必须跳过未回传的历史停用 SKU 编号。
    #[test]
    fn append_numbering_skips_disabled_skus() {
        let (product, row, revision, existing) = fixture();
        let mut request =
            append_request(&product, &row, revision, &[], vec![existing], RowMedia::default()).unwrap();
        // -01、-03 已停用，只回传启用的 -02；商品服务读取的占用集合包含全部历史编号。
        let occupied = (1..=3).map(|index| format!("{}-{index:02}", product.product_no));
        ProductSkuInput::fill_blank_sku_nos(&product.product_no, occupied, &mut request.skus);
        assert_eq!(request.skus[0].sku_no, format!("{}-02", product.product_no));
        assert_eq!(request.skus[1].sku_no, "SKU20261026-000001-04");
    }

    /// 无可继承单位时明确失败，不创建缺少单位的新 SKU。
    #[test]
    fn append_rejects_missing_existing_sku() {
        let (product, row, revision, _) = fixture();
        let error =
            append_request(&product, &row, revision, &[], Vec::new(), RowMedia::default()).unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message) if message == "商品没有可继承的计量单位"));
    }
}
