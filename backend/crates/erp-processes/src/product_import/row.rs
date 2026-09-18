//! 导入单行：创建实物商品 + 单 SKU。

use application_core::AuditActor;
use erp_catalog::entity::catalog::{EnableStatus, ProductKind};
use erp_catalog::repository::prelude::*;
use erp_catalog::{CatalogExt, CreateProductRequest, ProductMediaInput, ProductSkuInput};
use erp_core::common::time::BusinessDate;
use persistence_core::NoTransaction;

use super::ProductImportProcess;
use super::identity::normalize_import_row;
use super::images::{RowMediaSource, resolve_row_media};
use super::resolve::ImportDictionaryCache;
use crate::{Error, Result};

/// 单行导入结果。
#[derive(Debug, Clone)]
pub struct RowImportOutcome {
    /// 成功或跳过时的商品 ID。
    pub product_id: Option<String>,
    /// 结果说明。
    pub message: String,
    /// 是否跳过（已存在）。
    pub skipped: bool,
}

impl RowImportOutcome {
    /// 构造单行导入结果。
    ///
    /// # 参数
    /// * `message` - 结果说明
    ///
    /// # 返回
    /// 返回未跳过、商品为空的结果。
    ///
    /// # 错误
    /// 无。
    pub fn new(message: String) -> Self {
        Self { product_id: None, message, skipped: false }
    }

    /// 设置商品 ID。
    ///
    /// # 参数
    /// * `product_id` - 成功或跳过时的商品 ID
    ///
    /// # 返回
    /// 返回更新后的结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_product_id(mut self, product_id: Option<String>) -> Self {
        self.product_id = product_id;
        self
    }

    /// 设置是否跳过。
    ///
    /// # 参数
    /// * `skipped` - 是否跳过
    ///
    /// # 返回
    /// 返回更新后的结果。
    ///
    /// # 错误
    /// 无。
    pub fn with_skipped(mut self, skipped: bool) -> Self {
        self.skipped = skipped;
        self
    }
}

impl ProductImportProcess {
    /// 导入报价表中的一行商品。
    ///
    /// # 参数
    /// * `row_number` - Excel 行号
    /// * `cells` - 单元格
    /// * `media_source` - 行媒体来源（清单复用或源文件提取）
    /// * `actor` - 审计操作人
    /// * `cache` - 字典缓存
    ///
    /// # 返回
    /// 返回成功、跳过或失败说明。
    ///
    /// # 错误
    /// 校验失败或写入失败时返回错误。
    pub(super) async fn import_row(
        &self,
        row_number: u32,
        cells: &[String],
        media_source: &RowMediaSource<'_>,
        actor: &AuditActor,
        cache: &mut ImportDictionaryCache,
    ) -> Result<RowImportOutcome> {
        let row = normalize_import_row(row_number, cells)?;
        if let Some(existing) =
            self.db.products().find_by_product_no(&row.product_no, &mut NoTransaction).await?
        {
            if row.coded_spu {
                return self.import_sku_into_spu(existing, row, cells, media_source, actor).await;
            }
            return self.import_existing_with_images(existing, cells, media_source, actor).await;
        }
        if let Some(barcode) = &row.barcode {
            let owners = self.db.catalog().barcode_owner_sku_ids(barcode, &mut NoTransaction).await?;
            if !owners.is_empty() {
                return Ok(RowImportOutcome::new(format!("条码 {barcode} 已被其他商品使用，本行未写入"))
                    .with_skipped(true));
            }
        }
        let unit_id = self.resolve_unit(cache).await?;
        let brand_id = self.resolve_brand(&row.brand_name, actor, cache).await?;
        let category_id = self.resolve_category(&row.category_name, actor, cache).await?;
        let media = resolve_row_media(&self.storage, &self.secret, cells, media_source).await?;
        let carousel_media = media
            .carousel
            .iter()
            .map(|(id, sort_order)| ProductMediaInput {
                file_asset_id: id.clone(),
                sort_order: *sort_order,
                alt_text: Some(row.name.clone()),
            })
            .collect();
        let request = CreateProductRequest {
            change_reason: Some(format!("产品报价表导入 第{}行", row.row_number)),
            product_no: row.product_no,
            product_kind: ProductKind::Physical,
            maintainer_user_id: None,
            name: row.name.clone(),
            description: None,
            specification: row.specification,
            category_id,
            brand_id,
            status: Some(EnableStatus::Active),
            effective_from: BusinessDate::today(),
            effective_to: None,
            carousel_media,
            detail_media: Vec::new(),
            skus: vec![ProductSkuInput {
                sku_id: None,
                expected_sku_revision_id: None,
                reenable: false,
                sku_no: row.sku_no,
                name: row.name,
                base_unit_id: unit_id,
                barcode: row.barcode,
                main_image_asset_id: media.main_image,
                weight_kg: None,
                volume_m3: None,
                sales_visible_price_gross: row.sales_price,
                market_price: row.market_price,
                spec_entries: row.spec_entries,
            }],
        };
        let created = crate::product_create_with_assets(
            self.db.clone(),
            self.rbac.clone(),
            request,
            media.pending,
            actor.clone(),
        )
        .await;
        match created {
            Ok(view) => Ok(RowImportOutcome::new("商品已导入".into()).with_product_id(Some(view.id))),
            Err(Error::ConflictError(_)) => {
                let existing = self
                    .db
                    .products()
                    .find_by_product_no(
                        &normalize_import_row(row_number, cells)?.product_no,
                        &mut NoTransaction,
                    )
                    .await?;
                Ok(RowImportOutcome::new("商品已存在，本行未重复写入".into())
                    .with_product_id(existing.map(|item| item.base.id))
                    .with_skipped(true))
            },
            Err(error) => Err(error),
        }
    }
}
