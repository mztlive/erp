//! Named catalog processes that own audited outer transactions.

use crate::Result;
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_catalog::entity::catalog::product_category::{ProductCategory, ProductCategoryData};
use erp_catalog::entity::catalog::sku_attribute::{SkuAttribute, SkuAttributeData};
use erp_catalog::entity::catalog::sku_attribute_value::{SkuAttributeValue, SkuAttributeValueData};
use erp_catalog::entity::catalog::unit_of_measure::{UnitOfMeasure, UnitOfMeasureData};
use erp_catalog::entity::catalog::{
    EnableStatus, ProductCategoryId, SkuAttributeId, SkuAttributeValueId, UnitOfMeasureId,
};
use erp_catalog::{
    CatalogExt, CreateProductCategoryRequest, CreateSkuAttributeRequest, CreateSkuAttributeValueRequest,
    CreateUnitOfMeasureRequest, ProductCategoryView, SkuAttributeValueView, SkuAttributeView,
    UnitOfMeasureView,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::adapters::catalog_service;
use crate::audit::run_audited;

/// Process module name.
pub fn process_name() -> &'static str {
    "catalog"
}

/// Create a unit of measure and persist the success audit in one transaction.
pub async fn create_unit_of_measure(
    db: Database,
    req: CreateUnitOfMeasureRequest,
    actor: AuditActor,
) -> Result<UnitOfMeasureView> {
    req.validate()?;
    let id = UnitOfMeasureId::new(next_id());
    let unit = UnitOfMeasure::new(
        id.clone(),
        UnitOfMeasureData {
            unit_code: req.unit_code,
            name: req.name,
            symbol: req.symbol,
            quantity_scale: req.quantity_scale,
            status: req.status.unwrap_or(EnableStatus::Active),
        },
        actor.id(),
    )?;
    let audit = actor
        .clone()
        .resource_log("unit_of_measure.create", "unit_of_measure", id.to_string())?;
    let unit_for_tx = unit.clone();
    run_audited(&db, audit, move |db, session| {
        Box::pin(async move {
            db.unit_of_measures().create(&unit_for_tx, session).await?;
            Ok(())
        })
    })
    .await?;
    Ok(unit.into())
}

/// Create a product category and persist the success audit in one transaction.
pub async fn create_product_category(
    db: Database,
    req: CreateProductCategoryRequest,
    actor: AuditActor,
) -> Result<ProductCategoryView> {
    req.validate()?;
    let parent_id = req.parent_category_id.clone();
    let id = ProductCategoryId::new(next_id());
    catalog_service(db.clone())
        .ensure_parent_chain_ok(id.as_ref(), parent_id.as_ref())
        .await?;
    let category = ProductCategory::new(
        id.clone(),
        ProductCategoryData {
            category_code: req.category_code,
            parent_category_id: parent_id,
            name: req.name,
            product_kind: req.product_kind,
            status: req.status.unwrap_or(EnableStatus::Active),
        },
        actor.id(),
    )?;
    let audit = actor
        .clone()
        .resource_log("product_category.create", "product_category", id.to_string())?;
    let category_for_tx = category.clone();
    run_audited(&db, audit, move |db, session| {
        Box::pin(async move {
            db.product_categories().create(&category_for_tx, session).await?;
            Ok(())
        })
    })
    .await?;
    Ok(category.into())
}

/// Create a SKU attribute and persist the success audit in one transaction.
pub async fn create_sku_attribute(
    db: Database,
    req: CreateSkuAttributeRequest,
    actor: AuditActor,
) -> Result<SkuAttributeView> {
    req.validate()?;
    let id = SkuAttributeId::new(next_id());
    let attribute = SkuAttribute::new(
        id.clone(),
        SkuAttributeData {
            attribute_code: req.attribute_code,
            name: req.name,
            value_type: req.value_type,
            status: req.status.unwrap_or(EnableStatus::Active),
        },
        actor.id(),
    )?;
    let audit = actor
        .clone()
        .resource_log("sku_attribute.create", "sku_attribute", id.to_string())?;
    let attribute_for_tx = attribute.clone();
    run_audited(&db, audit, move |db, session| {
        Box::pin(async move {
            db.sku_attributes().create(&attribute_for_tx, session).await?;
            Ok(())
        })
    })
    .await?;
    Ok(attribute.into())
}

/// Create a SKU attribute value and persist the success audit in one transaction.
pub async fn create_sku_attribute_value(
    db: Database,
    req: CreateSkuAttributeValueRequest,
    actor: AuditActor,
) -> Result<SkuAttributeValueView> {
    req.validate()?;
    catalog_service(db.clone())
        .load_attribute(req.attribute_id.as_ref())
        .await?;
    let id = SkuAttributeValueId::new(next_id());
    let value = SkuAttributeValue::new(
        id.clone(),
        SkuAttributeValueData {
            attribute_id: req.attribute_id,
            value_code: req.value_code,
            display_value: req.display_value,
            sort_order: req.sort_order,
            status: req.status.unwrap_or(EnableStatus::Active),
        },
        actor.id(),
    )?;
    let audit = actor.clone().resource_log(
        "sku_attribute_value.create",
        "sku_attribute_value",
        id.to_string(),
    )?;
    let value_for_tx = value.clone();
    run_audited(&db, audit, move |db, session| {
        Box::pin(async move {
            db.sku_attribute_values().create(&value_for_tx, session).await?;
            Ok(())
        })
    })
    .await?;
    Ok(value.into())
}
