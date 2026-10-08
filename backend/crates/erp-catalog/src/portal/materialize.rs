//! 商品物化加入同一流程事务，且不更新被复用的资料。

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    FileAssetId, ProductId, ProductRevisionId, ProductRevisionMediaId, SkuId, SkuRevisionId, UnitOfMeasureId,
};
use id_generator::next_id;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::{
    CatalogDraftResult, CatalogDraftSkuResult, CatalogPortalService, DraftSku, NewProductDraft,
    NewProductInput, NormalizedProduct, NormalizedSku,
};
use crate::entity::catalog::product::ProductData;
use crate::entity::catalog::product_revision::ProductRevisionData;
use crate::entity::catalog::product_revision_media::{
    MediaRole, ProductRevisionMedia, ProductRevisionMediaData,
};
use crate::entity::catalog::sku::SkuData;
use crate::entity::catalog::sku_revision::SkuRevisionData;
use crate::repository::CatalogExt;
use crate::{
    EnableStatus, Error, ListingStatus, Product, ProductRevision, Result, Sku, SkuRevision,
    SpecSignatureEntry, compute_specification_signature,
};

/// Explicitly confirmed product identity; there is no name-based auto-merge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingProductRef {
    pub product_id: String,
    pub version: u64,
    pub revision_id: String,
}

/// Frozen review intent provided by the authorized internal process.
pub struct CatalogMaterializeCommand {
    pub draft_id: String,
    pub expected_version: u64,
    pub normalized: NormalizedProduct,
    pub product_target: Option<ExistingProductRef>,
    pub maintainer_user_id: String,
    pub business_org_unit_id: String,
    pub actor: AuditActor,
}

impl CatalogPortalService {
    /// 缺少商品身份时新建，已给出版本的身份则原样复用。
    /// # 参数
    /// `command` 为内部核对结果，`executor` 为商品、供给、任务共同使用的事务。
    /// # 返回
    /// 精确商品和逐SKU结果；新建SKU始终未上架且没有销售价。
    /// # 错误
    /// 申请或字典变化、实质内容改动、对象越界、规格或条码冲突。
    pub async fn materialize(
        &self,
        command: &CatalogMaterializeCommand,
        executor: &mut dyn Executor,
    ) -> Result<CatalogDraftResult> {
        let draft = self.load(&command.draft_id, executor).await?;
        ensure_version(draft.base.version, command.expected_version)?;
        let input = draft.submitted()?;
        input.ensure_submission_images()?;
        self.ensure_dictionaries(input, &command.normalized, executor).await?;
        let (product, product_created) = self.resolve_product(&draft, command, executor).await?;
        let mut skus = Vec::with_capacity(input.skus.len());
        for row in &input.skus {
            let mapped = command
                .normalized
                .sku_mappings
                .iter()
                .find(|sku| sku.row_id == row.row_id)
                .ok_or_else(|| Error::ValidationError("SKU映射缺失".into()))?;
            skus.push(self.resolve_sku(&product, row, mapped, &command.actor, executor).await?);
        }
        Ok(CatalogDraftResult { product_id: product.base.id, product_created, skus })
    }

    async fn resolve_product(
        &self,
        draft: &NewProductDraft,
        command: &CatalogMaterializeCommand,
        executor: &mut dyn Executor,
    ) -> Result<(Product, bool)> {
        let input = draft.submitted()?;
        if let Some(target) = &command.product_target {
            let product = self
                .catalog
                .access()
                .require_product(&command.actor, "update", &target.product_id, executor)
                .await?;
            ensure_version(product.base.version, target.version)?;
            if !product.is_active()
                || product.product_kind != input.product_kind
                || product.stable.current_revision_id.as_ref().map(ToString::to_string).as_deref()
                    != Some(&target.revision_id)
            {
                return Err(Error::ConflictError("匹配商品已变化或不可用".into()));
            }
            let revision = self
                .db
                .catalog()
                .current_product_revision(&product, executor)
                .await?
                .ok_or_else(|| Error::ConflictError("匹配商品当前资料缺失".into()))?;
            if revision.brand_id.as_ref() != command.normalized.brand_id
                || revision.category_id.as_ref() != command.normalized.category_id
            {
                return Err(Error::BusinessLogicError("匹配商品的品牌或分类与内部确认映射不一致".into()));
            }
            product.ensure_has_responsibility()?;
            return Ok((product, false));
        }
        let (owner, org) = self
            .catalog
            .access()
            .maintainer_org(Some(&command.maintainer_user_id), &command.actor, executor)
            .await?;
        if org != command.business_org_unit_id {
            return Err(Error::ConflictError("商品维护人主属组织已变化".into()));
        }
        self.catalog.access().ensure_writable(&command.actor, "create", &owner, &org, executor).await?;
        self.create_product(input, &command.normalized, owner, org, &command.actor, executor)
            .await
            .map(|product| (product, true))
    }

    async fn create_product(
        &self,
        input: &NewProductInput,
        mapped: &NormalizedProduct,
        owner: String,
        org: String,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Product> {
        let product_id = ProductId::new(next_id());
        let mut product = Product::new(
            product_id.clone(),
            ProductData {
                product_no: format!("P-{}", product_id.as_ref()),
                product_kind: input.product_kind,
                status: EnableStatus::Active,
                maintainer_user_id: owner,
                business_org_unit_id: org,
            },
            actor.id(),
        )?;
        let revision = ProductRevision::new(
            ProductRevisionId::new(next_id()),
            ProductRevisionData {
                product_id,
                revision_no: 1,
                name: mapped.name.clone(),
                description: input.description.clone(),
                specification: input.model.clone(),
                category_id: mapped.category_id.clone().into(),
                brand_id: mapped.brand_id.clone().into(),
                status: EnableStatus::Active,
                effective_from: BusinessDate::today(),
                effective_to: None,
            },
        )?;
        product.attach_revision(&revision, actor.id())?;
        let media = product_media(input, &revision)?;
        self.db.products().create(&product, executor).await?;
        self.db.catalog().create_product_revision_with_media(&revision, &media, executor).await?;
        Ok(product)
    }

    async fn resolve_sku(
        &self,
        product: &Product,
        row: &DraftSku,
        mapped: &NormalizedSku,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CatalogDraftSkuResult> {
        let signature = signature(row)?;
        if let Some(target) = &mapped.target_sku {
            let sku = self
                .db
                .skus()
                .find_by_id(&target.sku_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("匹配SKU不存在".into()))?;
            ensure_version(sku.base.version, target.version)?;
            if !sku.is_active()
                || sku.product_id.as_ref() != product.base.id
                || sku.base_unit_id.as_ref() != mapped.unit_id
                || sku.specification_signature != signature
                || sku.stable.current_revision_id.as_ref().map(ToString::to_string).as_deref()
                    != Some(&target.revision_id)
            {
                return Err(Error::ConflictError("匹配SKU身份、单位、规格或修订已变化".into()));
            }
            let revision = self
                .db
                .sku_revisions()
                .find_by_id(&target.revision_id, executor)
                .await?
                .ok_or_else(|| Error::ConflictError("匹配SKU修订不存在".into()))?;
            if row
                .barcode
                .as_deref()
                .map(str::trim)
                .filter(|code| !code.is_empty())
                .is_some_and(|barcode| Some(barcode) != revision.barcode.as_deref())
            {
                return Err(Error::BusinessLogicError("匹配SKU条码不一致，须重新核对".into()));
            }
            self.db.catalog().claim_sku_barcode(&revision, executor).await?;
            return Ok(sku_result(row, &sku, &revision, false));
        }
        if let Some(barcode) = row.barcode.as_deref().map(str::trim).filter(|code| !code.is_empty())
            && !self.db.catalog().barcode_owner_sku_ids(barcode, executor).await?.is_empty()
        {
            return Err(Error::ConflictError("条码已被SKU占用，请明确匹配后重试".into()));
        }
        let (sku, revision) = new_sku(product, row, mapped, &signature, actor.id())?;
        self.db.catalog().create_sku_with_revision(&sku, &revision, &[], executor).await?;
        Ok(sku_result(row, &sku, &revision, true))
    }
}

fn signature(row: &DraftSku) -> Result<String> {
    let entries = row
        .spec_entries
        .iter()
        .map(|entry| SpecSignatureEntry {
            attribute_code: entry.attribute_code.clone(),
            value_code: entry.attribute_value_code.clone(),
        })
        .collect::<Vec<_>>();
    Ok(compute_specification_signature(&entries)?)
}

fn new_sku(
    product: &Product,
    row: &DraftSku,
    mapped: &NormalizedSku,
    signature: &str,
    actor_id: &str,
) -> Result<(Sku, SkuRevision)> {
    let sku_id = SkuId::new(next_id());
    let mut sku = Sku::new(
        sku_id.clone(),
        SkuData {
            sku_no: format!("SKU-{}", sku_id.as_ref()),
            product_id: ProductId::new(product.base.id.clone()),
            base_unit_id: UnitOfMeasureId::new(&mapped.unit_id),
            specification_signature: signature.into(),
            status: EnableStatus::Active,
            listing_status: ListingStatus::Unlisted,
        },
        actor_id,
    )?;
    let revision = SkuRevision::new(
        SkuRevisionId::new(next_id()),
        SkuRevisionData {
            sku_id,
            revision_no: 1,
            name: mapped.name.clone(),
            description: None,
            specification: None,
            barcode: row.barcode.clone(),
            source_main_image_asset_id: row.image_asset_id.as_ref().map(FileAssetId::new),
            weight_kg: None,
            volume_m3: None,
            factory_price_gross: None,
            sales_visible_price_gross: None,
            bulk_price_gross: None,
            bulk_min_quantity: None,
            market_price: None,
            status: EnableStatus::Active,
            effective_from: BusinessDate::today(),
            effective_to: None,
        },
    )?;
    sku.attach_revision(&revision, actor_id)?;
    Ok((sku, revision))
}

fn product_media(input: &NewProductInput, revision: &ProductRevision) -> Result<Vec<ProductRevisionMedia>> {
    input
        .image_asset_ids
        .iter()
        .enumerate()
        .map(|(position, asset_id)| {
            let sort_order =
                i32::try_from(position).map_err(|_| Error::ValidationError("商品图片过多".into()))?;
            Ok(ProductRevisionMedia::new(
                ProductRevisionMediaId::new(next_id()),
                ProductRevisionMediaData {
                    product_revision_id: ProductRevisionId::new(revision.base.id.clone()),
                    file_asset_id: FileAssetId::new(asset_id),
                    media_role: MediaRole::Carousel,
                    sort_order,
                    alt_text: Some(input.name.clone()),
                },
            )?)
        })
        .collect()
}

fn sku_result(row: &DraftSku, sku: &Sku, revision: &SkuRevision, created: bool) -> CatalogDraftSkuResult {
    CatalogDraftSkuResult {
        row_id: row.row_id.clone(),
        sku_id: sku.base.id.clone(),
        revision_id: revision.base.id.clone(),
        sku_created: created,
        listing_status: sku.listing_status,
        offering_id: None,
    }
}

fn ensure_version(actual: u64, expected: u64) -> Result<()> {
    if actual != expected {
        return Err(Error::ConflictError("申请或匹配对象已变化，请重新核对".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use serde_json::json;

    use super::*;
    use crate::portal::DictionaryInput;

    fn row() -> DraftSku {
        DraftSku {
            row_id: "r1".into(),
            name: "茶叶礼盒".into(),
            spec_entries: vec![],
            unit: DictionaryInput { raw_name: "盒".into(), selected_id: None, expected_version: None },
            barcode: None,
            image_asset_id: Some("image-1".into()),
            ordering_code: "SUPPLIER-CODE".into(),
            supply_terms: json!({"dropship_price_gross":"50.00"}),
            packaging: None,
            quote_basis: None,
            available_quantity: None,
            reported_at: Instant::from_unix_secs(100),
        }
    }

    #[test]
    fn new_sku_is_unlisted_without_any_company_sales_price() {
        let product = Product::new(
            ProductId::new("product"),
            ProductData {
                product_no: "P-1".into(),
                product_kind: crate::ProductKind::Physical,
                status: EnableStatus::Active,
                maintainer_user_id: "buyer".into(),
                business_org_unit_id: "org".into(),
            },
            "reviewer",
        )
        .unwrap();
        let mapped = NormalizedSku {
            row_id: "r1".into(),
            name: "茶叶礼盒".into(),
            unit_id: "unit".into(),
            unit_version: 1,
            unit_synonym_confirmation: None,
            target_sku: None,
        };
        let (sku, revision) = new_sku(&product, &row(), &mapped, "", "reviewer").unwrap();
        assert_eq!(sku.listing_status, ListingStatus::Unlisted);
        assert_ne!(sku.sku_no, "SUPPLIER-CODE");
        assert!(revision.sales_visible_price_gross.is_none());
        assert!(revision.factory_price_gross.is_none());
        assert!(revision.bulk_price_gross.is_none());
        assert!(revision.market_price.is_none());
        assert_eq!(revision.source_main_image_asset_id.as_ref().map(|id| id.as_ref()), Some("image-1"));
    }

    #[test]
    fn materialization_result_preserves_reused_listing_state() {
        let product = Product::new(
            ProductId::new("product"),
            ProductData {
                product_no: "P-1".into(),
                product_kind: crate::ProductKind::Physical,
                status: EnableStatus::Active,
                maintainer_user_id: "buyer".into(),
                business_org_unit_id: "org".into(),
            },
            "reviewer",
        )
        .unwrap();
        let mapped = NormalizedSku {
            row_id: "r1".into(),
            name: "茶叶礼盒".into(),
            unit_id: "unit".into(),
            unit_version: 1,
            unit_synonym_confirmation: None,
            target_sku: None,
        };
        let (mut sku, revision) = new_sku(&product, &row(), &mapped, "", "reviewer").unwrap();
        sku.set_listing_status(ListingStatus::Listed, "buyer").unwrap();
        let result = sku_result(&row(), &sku, &revision, false);
        assert_eq!(result.listing_status, ListingStatus::Listed);
        assert!(!result.sku_created);
        assert_eq!(sku.listing_status, ListingStatus::Listed);
    }
}
