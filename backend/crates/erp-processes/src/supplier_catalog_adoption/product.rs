use application_core::AuditActor;
use entity_core::BaseModel;
use erp_catalog::repository::prelude::*;
use erp_catalog::service::catalog::ProductDraft;
use erp_catalog::{CatalogExt, CreateProductBrandRequest, ProductSkuInput, Sku};
use erp_core::ids::ProductId;
use erp_supply::catalog_adoption::{
    AdoptionRow, AdoptionTarget, CatalogAdoptionExt, CatalogAdoptionProduct, adoption_identity,
};
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use persistence_core::{Executor, NoTransaction};
use sha2::{Digest, Sha256};

use super::SupplierCatalogAdoptionProcess;
use super::request::{product_request, sku_input};
use crate::adapters::{catalog_access, catalog_service, scoped_catalog_service};
use crate::source_media::PreparedSourceMedia;
use crate::{Error, Result};

pub(super) struct PreparedAdoption {
    pub media: PreparedSourceMedia,
    pub materialized_item: SupplierCatalogItem,
    pub draft: Option<ProductDraft>,
    pub sku_input: Option<ProductSkuInput>,
    pub resolved_row: AdoptionRow,
}

impl SupplierCatalogAdoptionProcess {
    pub(super) async fn prepare_row(
        &self,
        item: &SupplierCatalogItem,
        row: &AdoptionRow,
        actor: &AuditActor,
    ) -> Result<PreparedAdoption> {
        let media = PreparedSourceMedia::from_archived(item);
        let cleanup = media.cleanup_handle();
        let outcome = self.prepare_row_with_media(item, row, actor, media).await;
        if outcome.is_err() {
            cleanup.discard_uncommitted().await;
        }
        outcome
    }

    async fn prepare_row_with_media(
        &self,
        item: &SupplierCatalogItem,
        row: &AdoptionRow,
        actor: &AuditActor,
        media: PreparedSourceMedia,
    ) -> Result<PreparedAdoption> {
        let mut materialized_item = item.clone();
        media.apply_to(&mut materialized_item)?;
        let mut resolved_row = row.clone();
        if let AdoptionTarget::Create { brand_id, .. } = &mut resolved_row.target
            && brand_id.is_empty()
        {
            *brand_id = self.resolve_source_brand(item, actor).await?;
        }
        let (draft, input) = if matches!(resolved_row.target, AdoptionTarget::Create { .. }) {
            let request = product_request(item, &resolved_row, &media)?;
            let input = sku_input(item, &resolved_row, &media)?;
            let mapping = self
                .db
                .catalog_adoption_products()
                .find_by_id(&product_binding_id(item), &mut NoTransaction)
                .await?;
            let draft = match mapping {
                Some(_) => None,
                None => Some(
                    scoped_catalog_service(self.db.clone(), self.rbac.clone())
                        .prepare_product_create(request, &media.pending_assets, actor)
                        .await?,
                ),
            };
            (draft, Some(input))
        } else {
            (None, None)
        };
        Ok(PreparedAdoption { media, materialized_item, draft, sku_input: input, resolved_row })
    }

    async fn resolve_source_brand(&self, item: &SupplierCatalogItem, actor: &AuditActor) -> Result<String> {
        let name = source_brand_name(item.brand_name.as_deref());
        if let Some(brand) =
            self.db.product_brands().find_enabled_by_exact_name(name, &mut NoTransaction).await?
        {
            return Ok(brand.base.id);
        }
        let mut hasher = Sha256::new();
        hasher.update(name.as_bytes());
        hasher.update([0u8]);
        let code = format!("BRD-{}", hex::encode(&hasher.finalize()[..6])).to_uppercase();
        match catalog_service(self.db.clone())
            .product_brand_create(CreateProductBrandRequest::new(code, name), actor)
            .await
        {
            Ok(brand) => Ok(brand.id),
            Err(erp_catalog::Error::ConflictError(_)) => self
                .db
                .product_brands()
                .find_enabled_by_exact_name(name, &mut NoTransaction)
                .await?
                .map(|brand| brand.base.id)
                .ok_or_else(|| Error::ConflictError("品牌创建冲突，请检查品牌是否停用后重试".into())),
            Err(error) => Err(error.into()),
        }
    }

    pub(super) async fn materialize_target(
        &self,
        item: &SupplierCatalogItem,
        row: &AdoptionRow,
        prepared: &mut PreparedAdoption,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Sku> {
        let mapping =
            self.db.catalog_adoption_products().find_by_id(&product_binding_id(item), executor).await?;
        let sku = match &row.target {
            AdoptionTarget::Existing {
                sku_id,
                expected_sku_version,
                expected_product_version,
                unit_id,
                ..
            } => {
                let sku = self
                    .db
                    .skus()
                    .find_by_id(sku_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("公司商品规格不存在".into()))?;
                let product = catalog_access(self.db.clone(), self.rbac.clone())
                    .require_product(actor, "update", sku.product_id.as_ref(), executor)
                    .await?;
                if !sku.is_active()
                    || !product.is_active()
                    || sku.base.version != *expected_sku_version
                    || product.base.version != *expected_product_version
                    || sku.base_unit_id.as_ref() != unit_id
                {
                    return Err(Error::ConflictError("公司商品规格、单位或版本已变化".into()));
                }
                prepared.media.pending_assets.persist(&self.db, executor).await?;
                sku
            },
            AdoptionTarget::Create { .. } => {
                self.create_target(mapping.as_ref(), item, row, prepared, actor, executor).await?
            },
        };
        if let Some(mapping) = mapping {
            if mapping.product_id != sku.product_id.as_ref() {
                return Err(Error::ConflictError("同一来源商品已关联其他公司商品，请核对来源规格".into()));
            }
        } else {
            let mapping = CatalogAdoptionProduct {
                base: BaseModel::new(product_binding_id(item)),
                connection_id: item.connection_id.clone(),
                source_product_id: item.product_id.clone(),
                product_id: sku.product_id.to_string(),
            };
            self.db.catalog_adoption_products().create(&mapping, executor).await?;
        }
        Ok(sku)
    }

    async fn create_target(
        &self,
        mapping: Option<&CatalogAdoptionProduct>,
        item: &SupplierCatalogItem,
        row: &AdoptionRow,
        prepared: &mut PreparedAdoption,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Sku> {
        self.ensure_create_dictionaries(item, row, executor).await?;
        let service = scoped_catalog_service(self.db.clone(), self.rbac.clone());
        if let Some(mapping) = mapping {
            self.ensure_bound_product(mapping, item, row, actor, executor).await?;
            prepared.media.pending_assets.persist(&self.db, executor).await?;
            let mut input =
                prepared.sku_input.take().ok_or_else(|| Error::Internal("新规格准备结果缺失".into()))?;
            input.main_image_asset_id = prepared
                .media
                .carousel_media
                .iter()
                .find(|image| image.sort_order == 0)
                .map(|image| image.file_asset_id.clone());
            return Ok(service.append_source_sku(&mapping.product_id, input, actor, executor).await?);
        }
        let draft = prepared
            .draft
            .take()
            .ok_or_else(|| Error::ConflictError("来源商品关联已变化，请重新确认".into()))?;
        let product = service
            .write_product_draft(draft, actor, prepared.media.pending_assets.clone(), executor)
            .await?;
        let mut skus = self.db.catalog().skus_for_product(&ProductId::new(product.base.id), executor).await?;
        if skus.len() != 1 {
            return Err(Error::Internal("选品新建规格结果不完整".into()));
        }
        Ok(skus.remove(0))
    }

    async fn ensure_create_dictionaries(
        &self,
        item: &SupplierCatalogItem,
        row: &AdoptionRow,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let AdoptionTarget::Create { category_id, brand_id, unit_id, .. } = &row.target else {
            return Ok(());
        };
        let request = product_request(item, row, &PreparedSourceMedia::empty())?;
        self.db
            .product_categories()
            .find_by_id(category_id, executor)
            .await?
            .filter(|category| category.is_active() && category.product_kind == request.product_kind)
            .ok_or_else(|| Error::ValidationError("公司分类已停用或不支持该商品类型".into()))?;
        let brand =
            self.db.product_brands().find_by_id(brand_id, executor).await?.filter(|brand| brand.is_active());
        let unit =
            self.db.unit_of_measures().find_by_id(unit_id, executor).await?.filter(|unit| unit.is_active());
        if brand.is_none() || unit.is_none() {
            return Err(Error::ValidationError("公司品牌或基础单位不存在或已停用".into()));
        }
        Ok(())
    }

    async fn ensure_bound_product(
        &self,
        mapping: &CatalogAdoptionProduct,
        item: &SupplierCatalogItem,
        row: &AdoptionRow,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let request = product_request(item, row, &PreparedSourceMedia::empty())?;
        let product = catalog_access(self.db.clone(), self.rbac.clone())
            .require_product(actor, "update", &mapping.product_id, executor)
            .await?;
        let revision = self
            .db
            .catalog()
            .current_product_revision(&product, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("已关联公司商品缺少当前资料".into()))?;
        if product.product_kind != request.product_kind
            || revision.category_id != request.category_id
            || revision.brand_id != request.brand_id
        {
            return Err(Error::ConflictError("同一来源商品的分类、品牌或业务类型映射不一致".into()));
        }
        Ok(())
    }
}

fn product_binding_id(item: &SupplierCatalogItem) -> String {
    adoption_identity(&[&item.connection_id, &item.product_id])
}

fn source_brand_name(name: Option<&str>) -> &str {
    name.map(str::trim).filter(|name| !name.is_empty()).unwrap_or("无品牌")
}

#[cfg(test)]
mod tests {
    use super::source_brand_name;

    #[test]
    fn source_brand_name_uses_supplier_name_or_unbranded_fallback() {
        assert_eq!(source_brand_name(Some("  山姆  ")), "山姆");
        assert_eq!(source_brand_name(None), "无品牌");
        assert_eq!(source_brand_name(Some("  ")), "无品牌");
    }
}
