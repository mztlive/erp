//! 联合导入：媒体在事务外准备，商品、SKU、供给和审计在同一事务提交。

use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::entity::catalog::Product;
use erp_catalog::ports::PendingAttachmentBatch;
use erp_catalog::repository::prelude::*;
use erp_catalog::service::catalog::{ProductDraft, SpecEditPlan};
use erp_catalog::{CatalogExt, ProductMediaInput};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{ProductId, SupplierAccountId};
use erp_file_assets::PendingFileAssets;
use erp_permissions::{catalog as catalog_permissions, supplier_offering as supplier_offering_permissions};
use erp_supplier::entity::supplier::eligibility::OfferingProductKind;
use erp_supplier::repository::SupplierExt;
use erp_supplier::repository::prelude::*;
use erp_supplier::service::supplier::eligibility::ensure_offering_capability_qualified;
use persistence_core::{Executor, ReadExecutor, Transactional};

use super::ProductImportProcess;
use super::identity::{NormalizedImportRow, normalize_import_row};
use super::images::{RowMedia, RowMediaSource, resolve_row_media};
use super::joint_commit::{ImportCommit, commit_row};
use super::resolve::ImportDictionaryCache;
use super::row::{RowImportOutcome, imported_product_request};
use super::supply_input::SupplyImportInput;
use crate::{Error, Result};

/// 尚未写入的目录变更；已有商品无需修改时保留稳定身份。
pub(super) enum ProductWrite {
    Create(ProductDraft),
    Update(SpecEditPlan),
    Existing(String),
}

/// 一行的目录计划与已上传附件。
pub(super) struct PreparedProduct {
    pub write: ProductWrite,
    pub pending: Arc<dyn PendingAttachmentBatch>,
}

impl ProductImportProcess {
    /// 新模板必须先证明供应商可用，再准备目录计划并原子提交。
    pub(super) async fn import_with_supply(
        &self,
        row_number: u32,
        cells: &[String],
        media_source: &RowMediaSource<'_>,
        actor: &AuditActor,
        cache: &mut ImportDictionaryCache,
    ) -> Result<RowImportOutcome> {
        let input = SupplyImportInput::from_cells(cells, BusinessDate::today())?;
        self.import_supplier(&input, actor, &mut ReadExecutor::default()).await?;

        let row = normalize_import_row(row_number, cells)?;
        let media = resolve_row_media(&self.storage, &self.secret, cells, media_source).await?;
        let prepared = self.prepare_supply_product(&row, media, actor, cache).await?;

        let process = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut commit = ImportCommit {
                        process: &process,
                        input: &input,
                        row: &row,
                        actor: &actor,
                        prepared: Some(prepared),
                    };
                    commit_row(&mut commit, executor).await
                })
            })
            .await
    }

    /// 按唯一编号读取已有供应商并复用实物供给资格、供给创建权限；事务内再次检查。
    pub(super) async fn import_supplier(
        &self,
        input: &SupplyImportInput,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierAccountId> {
        self.services
            .offering_access
            .clone()
            .ensure_writable(actor, &supplier_offering_permissions::CREATE, executor)
            .await?;
        let supplier =
            self.db.supplier_accounts().find_by_number(&input.supplier_no, executor).await?.ok_or_else(
                || {
                    Error::ValidationError(format!(
                        "供应商编号「{}」未录入，请先建立供应商资料",
                        input.supplier_no
                    ))
                },
            )?;
        let id = SupplierAccountId::new(supplier.base.id);
        ensure_offering_capability_qualified(
            &self.db,
            &id,
            OfferingProductKind::Physical,
            input.terms.valid_from.parse()?,
            executor,
        )
        .await?;
        Ok(id)
    }

    /// 准备新建或复用商品，供应商成本不会进入公司参考售价。
    async fn prepare_supply_product(
        &self,
        row: &NormalizedImportRow,
        media: RowMedia,
        actor: &AuditActor,
        cache: &mut ImportDictionaryCache,
    ) -> Result<PreparedProduct> {
        if let Some(product) =
            self.db.products().find_by_product_no(&row.product_no, &mut ReadExecutor::default()).await?
        {
            return self.prepare_existing_supply_product(product, row, media, actor).await;
        }

        let unit = self.resolve_unit(cache).await?;
        let brand = self.resolve_brand(&row.brand_name, actor, cache).await?;
        let category = self.resolve_category(&row.category_name, actor, cache).await?;

        let pending = PendingFileAssets::prepare(media.pending, actor)?.into_catalog_batch();
        let carousel = media
            .carousel
            .into_iter()
            .map(|(id, sort_order)| ProductMediaInput {
                file_asset_id: id,
                sort_order,
                alt_text: Some(row.name.clone()),
            })
            .collect();
        let request =
            imported_product_request(row.clone(), unit, brand, category, media.main_image, carousel);
        let draft =
            self.services.scoped_catalog.clone().prepare_product_create(request, &pending, actor).await?;
        Ok(PreparedProduct { write: ProductWrite::Create(draft), pending })
    }

    /// 已有规格仍处理供给，新增规格与补图使用目录领域的原编辑计划。
    async fn prepare_existing_supply_product(
        &self,
        product: Product,
        row: &NormalizedImportRow,
        media: RowMedia,
        actor: &AuditActor,
    ) -> Result<PreparedProduct> {
        self.services
            .catalog_access
            .clone()
            .require_product(
                actor,
                &catalog_permissions::PRODUCT_PRODUCT_DETAIL,
                &product.base.id,
                &mut ReadExecutor::default(),
            )
            .await?;

        let skus = self
            .db
            .skus()
            .find_by_product_ids(&[ProductId::new(product.base.id.clone())], &mut ReadExecutor::default())
            .await?;
        let revisions = self.db.catalog().current_sku_revisions(&skus, &mut ReadExecutor::default()).await?;
        let same = row.matching_sku(&skus, &revisions)?.is_some();
        if same && (media.pending.is_empty() || self.product_has_images(&product).await?) {
            let pending = PendingFileAssets::prepare(Vec::new(), actor)?.into_catalog_batch();
            return Ok(PreparedProduct { write: ProductWrite::Existing(product.base.id), pending });
        }

        let pending = PendingFileAssets::prepare(media.pending.clone(), actor)?.into_catalog_batch();
        let request = if same {
            self.update_request_with_images(&product, media).await?
        } else {
            self.append_sku_request(&product, row, &skus, media).await?
        };
        let plan = self
            .services
            .scoped_catalog
            .clone()
            .prepare_product_update(&product.base.id, request, &pending, actor)
            .await?;
        Ok(PreparedProduct { write: ProductWrite::Update(plan), pending })
    }
}
