//! 给已导入但缺图的商品补齐轮播图与 SKU 主图。

use application_core::AuditActor;
use erp_catalog::entity::catalog::{parse_specification_signature, EnableStatus, Product};
use erp_catalog::{CatalogExt, ProductMediaInput, ProductSkuInput, SpecEntryInput, UpdateProductRequest};
use erp_core::ids::{ProductId, SkuRevisionId};
use persistence_core::NoTransaction;

use super::images::{upload_row_images, RowMedia};
use super::parse::ParsedProductSheet;
use super::row::RowImportOutcome;
use super::ProductImportProcess;
use crate::{Error, Result};

impl ProductImportProcess {
    /// 已存在的商品若还没有图片，则用本行图片补齐。
    ///
    /// # 参数
    /// * `product` - 已存在商品
    /// * `cells` - 当前行单元格
    /// * `xlsx` - 源文件字节
    /// * `sheet` - 解析结果
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 补图成功、无需补图或仍跳过时的行结果。
    ///
    /// # 错误
    /// 读取现有商品、上传图片或更新失败时返回错误。
    pub(super) async fn import_existing_with_images(
        &self,
        product: Product,
        cells: &[String],
        xlsx: &[u8],
        sheet: &ParsedProductSheet,
        actor: &AuditActor,
    ) -> Result<RowImportOutcome> {
        if self.product_has_images(&product).await? {
            return Ok(already_imported(product.base.id));
        }
        let media = upload_row_images(&self.storage, &self.secret, xlsx, sheet, cells).await?;
        if media.pending.is_empty() {
            return Ok(already_imported(product.base.id));
        }
        let pending = media.pending.clone();
        let request = self.update_request_with_images(&product, media).await?;
        match crate::product_update_with_assets(
            self.db.clone(),
            product.base.id.clone(),
            request,
            pending,
            actor.clone(),
        )
        .await
        {
            Ok(_) => Ok(RowImportOutcome {
                product_id: Some(product.base.id),
                message: "商品已存在，已补齐图片".into(),
                skipped: false,
            }),
            Err(Error::ConflictError(_)) => Ok(already_imported(product.base.id)),
            Err(error) => Err(error),
        }
    }

    async fn product_has_images(&self, product: &Product) -> Result<bool> {
        let snapshot = self
            .db
            .catalog()
            .product_disable_snapshot(&product.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品不存在".into()))?;
        if !snapshot.media.is_empty() {
            return Ok(true);
        }
        let skus = self
            .db
            .skus()
            .find_by_product_ids(&[ProductId::new(product.base.id.clone())], &mut NoTransaction)
            .await?;
        let revisions = self
            .db
            .catalog()
            .current_sku_revisions(&skus, &mut NoTransaction)
            .await?;
        Ok(revisions
            .values()
            .any(|revision| revision.source_main_image_asset_id.is_some()))
    }

    async fn update_request_with_images(
        &self,
        product: &Product,
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
            .ok_or_else(|| Error::BusinessLogicError("商品缺少当前资料，无法补图".into()))?;
        let skus = self
            .db
            .skus()
            .find_by_product_ids(&[ProductId::new(product.base.id.clone())], &mut NoTransaction)
            .await?;
        let revisions = self
            .db
            .catalog()
            .current_sku_revisions(&skus, &mut NoTransaction)
            .await?;
        let sku_inputs = sku_inputs_with_main_image(&skus, &revisions, media.main_image.clone())?;
        Ok(UpdateProductRequest {
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
            carousel_media: media
                .carousel
                .iter()
                .map(|(id, sort_order)| ProductMediaInput {
                    file_asset_id: id.clone(),
                    sort_order: *sort_order,
                    alt_text: None,
                })
                .collect(),
            detail_media: Vec::new(),
            skus: sku_inputs,
        })
    }
}

fn already_imported(product_id: String) -> RowImportOutcome {
    RowImportOutcome {
        product_id: Some(product_id),
        message: "商品已存在，本行未重复写入".into(),
        skipped: true,
    }
}

/// 把当前启用 SKU 转成编辑请求中的保留行。
///
/// # 参数
/// * `skus` - 商品下 SKU
/// * `revisions` - 当前修订
/// * `main_image` - 可选覆盖到第一个 SKU 的主图
///
/// # 返回
/// 返回可提交的 SKU 行。
///
/// # 错误
/// 规格签名无法解析或没有启用 SKU 时返回错误。
pub(super) fn sku_inputs_with_main_image(
    skus: &[erp_catalog::entity::catalog::Sku],
    revisions: &std::collections::HashMap<String, erp_catalog::entity::catalog::SkuRevision>,
    main_image: Option<erp_core::ids::FileAssetId>,
) -> Result<Vec<ProductSkuInput>> {
    let mut inputs = Vec::new();
    let mut assigned_main = false;
    for sku in skus
        .iter()
        .filter(|sku| sku.stable.status == EnableStatus::Active)
    {
        let Some(revision) = revisions.get(&sku.base.id) else {
            continue;
        };
        let spec_entries = parse_specification_signature(&sku.specification_signature)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?
            .into_iter()
            .map(|entry| SpecEntryInput {
                attribute_code: entry.attribute_code,
                attribute_value_code: entry.value_code,
            })
            .collect();
        let main_image_asset_id = if assigned_main {
            revision.source_main_image_asset_id.clone()
        } else {
            assigned_main = true;
            main_image
                .clone()
                .or_else(|| revision.source_main_image_asset_id.clone())
        };
        inputs.push(ProductSkuInput {
            sku_id: Some(erp_core::ids::SkuId::new(sku.base.id.clone())),
            expected_sku_revision_id: Some(SkuRevisionId::new(
                sku.stable
                    .current_revision_id
                    .clone()
                    .unwrap_or_else(|| revision.base.id.clone()),
            )),
            reenable: false,
            sku_no: sku.sku_no.clone(),
            name: revision.name.clone(),
            base_unit_id: sku.base_unit_id.clone(),
            barcode: revision.barcode.clone(),
            main_image_asset_id,
            weight_kg: revision.weight_kg,
            volume_m3: revision.volume_m3,
            sales_visible_price_gross: revision.sales_visible_price_gross,
            market_price: revision.market_price,
            spec_entries,
        });
    }
    if inputs.is_empty() {
        return Err(Error::BusinessLogicError("商品没有可补图的 SKU".into()));
    }
    Ok(inputs)
}
