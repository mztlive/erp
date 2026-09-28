//! 生成或恢复演示商品。商品引用已经生成的单位、品牌和分类。

use std::collections::HashMap;
use std::str::FromStr;

use application_core::AuditActor;
use erp_catalog::{CatalogExt, CreateProductRequest, ProductKind, ProductSkuInput};
use erp_core::ids::{ProductBrandId, ProductCategoryId, UnitOfMeasureId};
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
        self.remember_product(step, id.to_string(), sku_ids).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
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
                label: format!("演示礼盒{:02}", step.ordinal),
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
        name: format!("演示礼盒{:02}", step.ordinal),
        description: Some("演示主数据".to_string()),
        specification: Some("标准装".to_string()),
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
            name: format!("演示礼盒{:02}", step.ordinal),
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
