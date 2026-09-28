//! 生成或恢复演示商品。商品引用已经生成的单位、品牌和分类。

use std::collections::HashMap;
use std::str::FromStr;

use application_core::AuditActor;
use erp_catalog::{CatalogExt, CreateProductRequest, ProductKind, ProductSkuInput, UpdateProductRequest};
use erp_core::ids::{ProductBrandId, ProductCategoryId, SkuId, SkuRevisionId, UnitOfMeasureId};
use erp_core::money::Amount;
use persistence_core::NoTransaction;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::{self, DemoStep};
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle};
use crate::{Error, Result};

impl DemoMasterDataService {
    pub(super) async fn ensure_product(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        records: &HashMap<String, DemoMasterRecord>,
    ) -> Result<EnsureOutcome> {
        let (unit_key, brand_key, category_key) = plan::product_links(step.ordinal);
        let unit_id = required_link(records, unit_key, "计量单位")?;
        let brand_id = required_link(records, &brand_key, "品牌")?;
        let category_id = required_link(records, &category_key, "分类")?;
        if let Some(existing) = self
            .db
            .products()
            .find_one_by_field_including_deleted("product_no", step.key.clone(), &mut NoTransaction)
            .await?
        {
            return self.adopt_product(actor, step, &existing.base.id, existing.base.deleted_at).await;
        }
        let view = self
            .catalog()
            .product_create(product_request(step, &unit_id, &brand_id, &category_id)?, actor)
            .await?;
        let sku_ids = self.sku_ids(&view.id).await?;
        self.remember_product(step, view.id, sku_ids).await?;
        Ok(EnsureOutcome::Created)
    }

    async fn adopt_product(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        id: &str,
        deleted_at: u64,
    ) -> Result<EnsureOutcome> {
        let sku_ids = self.sku_ids(id).await?;
        let live = deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        if !live {
            lifecycle::restore_product_graph(&self.db, actor, id, &sku_ids).await?;
        }
        self.refresh_product_name(actor, step, id).await?;
        self.remember_product(step, id.to_string(), sku_ids).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
    }

    async fn refresh_product_name(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        product_id: &str,
    ) -> Result<()> {
        let Some(product) =
            self.db.products().find_by_id_including_deleted(product_id, &mut NoTransaction).await?
        else {
            return Ok(());
        };
        let Some(revision_id) = product.stable.current_revision_id.as_deref() else {
            return Ok(());
        };
        let Some(revision) =
            self.db.product_revisions().find_by_id_including_deleted(revision_id, &mut NoTransaction).await?
        else {
            return Ok(());
        };
        if !revision.name.contains('演') {
            return Ok(());
        }
        let name = super::names::label(step);
        let skus = self.product_sku_inputs(product_id, name).await?;
        if skus.is_empty() {
            return Ok(());
        }
        self.catalog()
            .product_update(
                product_id,
                UpdateProductRequest {
                    version: product.base.version,
                    change_reason: Some("更新资料名称".to_string()),
                    name: name.to_string(),
                    description: Some(super::names::product_spec(step.ordinal).to_string()),
                    specification: Some(super::names::product_spec(step.ordinal).to_string()),
                    category_id: revision.category_id,
                    brand_id: revision.brand_id,
                    status: revision.status,
                    effective_from: revision.effective_from,
                    effective_to: revision.effective_to,
                    carousel_media: Vec::new(),
                    detail_media: Vec::new(),
                    skus,
                },
                actor,
            )
            .await?;
        Ok(())
    }

    async fn product_sku_inputs(&self, product_id: &str, name: &str) -> Result<Vec<ProductSkuInput>> {
        let skus = self
            .db
            .skus()
            .find_many_by_field_including_deleted("product_id", product_id.to_string(), &mut NoTransaction)
            .await?;
        let mut inputs = Vec::new();
        for sku in skus {
            if sku.base.deleted_at != entity_core::NOT_DELETED_TIMESTAMP {
                continue;
            }
            let Some(revision_id) = sku.stable.current_revision_id.as_deref() else {
                continue;
            };
            let Some(revision) =
                self.db.sku_revisions().find_by_id_including_deleted(revision_id, &mut NoTransaction).await?
            else {
                continue;
            };
            inputs.push(ProductSkuInput {
                sku_id: Some(SkuId::new(sku.base.id)),
                expected_sku_revision_id: Some(SkuRevisionId::new(revision.base.id)),
                reenable: false,
                sku_no: sku.sku_no,
                name: if revision.name.contains('演') { name.to_string() } else { revision.name },
                base_unit_id: sku.base_unit_id,
                barcode: revision.barcode,
                main_image_asset_id: revision.source_main_image_asset_id,
                weight_kg: revision.weight_kg,
                volume_m3: revision.volume_m3,
                sales_visible_price_gross: revision.sales_visible_price_gross,
                market_price: revision.market_price,
                spec_entries: Vec::new(),
            });
        }
        Ok(inputs)
    }

    async fn sku_ids(&self, product_id: &str) -> Result<Vec<String>> {
        let skus = self
            .db
            .skus()
            .find_many_by_field_including_deleted("product_id", product_id.to_string(), &mut NoTransaction)
            .await?;
        Ok(skus.into_iter().map(|sku| sku.base.id).collect())
    }

    async fn remember_product(&self, step: &DemoStep, id: String, sku_ids: Vec<String>) -> Result<()> {
        record::save(
            &self.db,
            &DemoMasterRecord {
                key: step.key.clone(),
                kind: step.kind.as_str().to_string(),
                entity_id: id,
                related_ids: sku_ids,
                label: super::names::label(step).to_string(),
                removed: false,
            },
        )
        .await
    }
}

fn required_link(records: &HashMap<String, DemoMasterRecord>, key: &str, label: &str) -> Result<String> {
    let record = records.get(key).ok_or_else(|| Error::ValidationError(format!("请先生成演示{label}")))?;
    if record.removed {
        return Err(Error::ValidationError(format!("演示{label}已删除，请重新生成")));
    }
    Ok(record.entity_id.clone())
}

fn product_request(
    step: &DemoStep,
    unit_id: &str,
    brand_id: &str,
    category_id: &str,
) -> Result<CreateProductRequest> {
    let price = Amount::from_str(&format!("{}.00", 80 + step.ordinal))
        .map_err(|error| Error::Internal(error.to_string()))?;
    Ok(CreateProductRequest {
        change_reason: Some("演示主数据".to_string()),
        product_no: step.key.clone(),
        product_kind: ProductKind::Physical,
        maintainer_user_id: None,
        name: super::names::label(step).to_string(),
        description: Some(super::names::product_spec(step.ordinal).to_string()),
        specification: Some(super::names::product_spec(step.ordinal).to_string()),
        category_id: ProductCategoryId::new(category_id),
        brand_id: ProductBrandId::new(brand_id),
        status: None,
        effective_from: super::demo_date()?,
        effective_to: None,
        carousel_media: Vec::new(),
        detail_media: Vec::new(),
        skus: vec![ProductSkuInput {
            sku_id: None,
            expected_sku_revision_id: None,
            reenable: false,
            sku_no: plan::sku_no(step.ordinal),
            name: super::names::label(step).to_string(),
            base_unit_id: UnitOfMeasureId::new(unit_id),
            barcode: None,
            main_image_asset_id: None,
            weight_kg: None,
            volume_m3: None,
            sales_visible_price_gross: Some(price),
            market_price: None,
            spec_entries: Vec::new(),
        }],
    })
}
