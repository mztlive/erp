//! 同一产品编码追加 SKU 到已有 SPU。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_catalog::entity::catalog::product_revision_media::MediaRole;
use erp_catalog::entity::catalog::{Product, Sku, SpecSignatureEntry, compute_specification_signature};
use erp_catalog::repository::prelude::*;
use erp_catalog::{CatalogExt, ProductMediaInput, ProductSkuInput, UpdateProductRequest};
use erp_core::ids::ProductId;
use persistence_core::NoTransaction;

use super::ProductImportProcess;
use super::backfill::sku_inputs_with_main_image;
use super::identity::{NormalizedImportRow, next_sku_no};
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

    async fn append_sku_request(
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
        let mut sku_inputs = sku_inputs_with_main_image(skus, &revisions, None)?;
        let taken = sku_inputs.iter().map(|sku| sku.sku_no.clone()).collect::<HashSet<_>>();
        sku_inputs.push(ProductSkuInput {
            sku_id: None,
            expected_sku_revision_id: None,
            reenable: false,
            sku_no: next_sku_no(&row.product_no, &taken),
            name: row.name.clone(),
            base_unit_id: sku_inputs
                .first()
                .map(|sku| sku.base_unit_id.clone())
                .ok_or_else(|| Error::BusinessLogicError("商品没有可继承的计量单位".into()))?,
            barcode: row.barcode.clone(),
            main_image_asset_id: media.main_image,
            weight_kg: None,
            volume_m3: None,
            sales_visible_price_gross: row.sales_price,
            market_price: row.market_price,
            spec_entries: row.spec_entries.clone(),
        });
        let mut carousel_media: Vec<ProductMediaInput> = snapshot
            .media
            .into_iter()
            .filter(|item| item.media_role == MediaRole::Carousel)
            .map(|item| ProductMediaInput {
                file_asset_id: item.file_asset_id,
                sort_order: item.sort_order,
                alt_text: item.alt_text,
            })
            .collect();
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
            detail_media: Vec::new(),
            skus: sku_inputs,
        })
    }
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
fn sku_signature(row: &NormalizedImportRow) -> Result<String> {
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
