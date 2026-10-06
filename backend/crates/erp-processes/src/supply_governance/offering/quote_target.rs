//! 在门户命令的同一执行器核对当前 SKU、商品修订和基础单位。

use async_trait::async_trait;
use erp_catalog::{CatalogExt, Product, Sku};
use erp_core::ids::SkuId;
use erp_supply::portal::QuoteTargetVersion;
use erp_supply::ports::offering_qualification::PortalQuoteQualificationPort;
use persistence_core::Executor;

use super::MongoOfferingQualification;
use crate::{Error, Result};

#[async_trait]
impl PortalQuoteQualificationPort for MongoOfferingQualification {
    async fn quote_target(&self, sku_id: &SkuId, executor: &mut dyn Executor) -> Result<QuoteTargetVersion> {
        let sku = self
            .db
            .skus()
            .find_by_id(sku_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("公司SKU不存在".into()))?;
        let product = self
            .db
            .products()
            .find_by_id(&sku.product_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("公司商品不存在".into()))?;
        let (sku_revision_id, product_revision_id) = target_revision_ids(&sku, &product)?;
        let sku_revision = self
            .db
            .sku_revisions()
            .find_by_id(sku_revision_id, executor)
            .await?
            .filter(|revision| revision.sku_id.as_ref() == sku.base.id && revision.is_active())
            .ok_or_else(|| Error::BusinessLogicError("公司SKU当前修订无效".into()))?;
        let product_revision = self
            .db
            .product_revisions()
            .find_by_id(product_revision_id, executor)
            .await?
            .filter(|revision| revision.product_id.as_ref() == product.base.id && revision.is_active())
            .ok_or_else(|| Error::BusinessLogicError("公司商品当前修订无效".into()))?;
        let unit = self
            .db
            .unit_of_measures()
            .find_by_id(&sku.base_unit_id, executor)
            .await?
            .filter(|unit| unit.is_active())
            .ok_or_else(|| Error::BusinessLogicError("SKU基础单位不存在或已停用".into()))?;
        Ok(QuoteTargetVersion {
            sku_version: sku.base.version,
            sku_revision_id: sku_revision.base.id,
            sku_revision_version: sku_revision.base.version,
            product_id: product.base.id,
            product_version: product.base.version,
            product_revision_id: product_revision.base.id,
            product_revision_version: product_revision.base.version,
            unit_id: unit.base.id,
            unit_version: unit.base.version,
        })
    }
}

/// 核对目标启用状态与正式指针，不以其他修订补齐缺失引用。
fn target_revision_ids<'a>(sku: &'a Sku, product: &'a Product) -> Result<(&'a str, &'a str)> {
    if !sku.is_active() || !product.is_active() || sku.product_id.as_ref() != product.base.id {
        return Err(Error::BusinessLogicError("公司SKU或所属商品未启用或引用无效".into()));
    }
    let sku_revision = sku
        .stable
        .current_revision_id
        .as_deref()
        .ok_or_else(|| Error::BusinessLogicError("公司SKU缺少当前正式修订".into()))?;
    let product_revision = product
        .stable
        .current_revision_id
        .as_deref()
        .ok_or_else(|| Error::BusinessLogicError("公司商品缺少当前正式修订".into()))?;
    Ok((sku_revision, product_revision))
}

#[cfg(test)]
mod tests {
    use erp_catalog::entity::catalog::product::ProductData;
    use erp_catalog::entity::catalog::sku::SkuData;
    use erp_catalog::{EMPTY_SPEC_SIGNATURE, EnableStatus, ListingStatus, ProductKind};
    use erp_core::ids::{ProductId, UnitOfMeasureId};

    use super::*;

    fn targets() -> (Sku, Product) {
        let mut product = Product::new(
            ProductId::new("product"),
            ProductData {
                product_no: "P001".into(),
                product_kind: ProductKind::Physical,
                status: EnableStatus::Active,
                maintainer_user_id: "owner".into(),
                business_org_unit_id: "org".into(),
            },
            "owner",
        )
        .unwrap();
        product.stable.current_revision_id = Some("product-revision".into());
        let mut sku = Sku::new(
            SkuId::new("sku"),
            SkuData {
                sku_no: "SKU001".into(),
                product_id: ProductId::new("product"),
                base_unit_id: UnitOfMeasureId::new("unit"),
                specification_signature: EMPTY_SPEC_SIGNATURE.into(),
                status: EnableStatus::Active,
                listing_status: ListingStatus::Unlisted,
            },
            "owner",
        )
        .unwrap();
        sku.stable.current_revision_id = Some("sku-revision".into());
        (sku, product)
    }

    #[test]
    fn quote_uses_current_formal_pointers_and_rejects_disabled_product_or_wrong_owner() {
        let (sku, product) = targets();
        assert_eq!(target_revision_ids(&sku, &product).unwrap(), ("sku-revision", "product-revision"));
        let mut stopped = product.clone();
        stopped.stable.status = EnableStatus::Disabled;
        assert!(target_revision_ids(&sku, &stopped).is_err());
        let mut foreign = product.clone();
        foreign.base.id = "other".into();
        assert!(target_revision_ids(&sku, &foreign).is_err());
    }

    #[test]
    fn missing_sku_or_product_pointer_rejects_instead_of_finding_another_revision() {
        let (mut sku, mut product) = targets();
        product.stable.current_revision_id = None;
        assert!(target_revision_ids(&sku, &product).is_err());
        product.stable.current_revision_id = Some("product-revision".into());
        sku.stable.current_revision_id = None;
        assert!(target_revision_ids(&sku, &product).is_err());
    }
}
