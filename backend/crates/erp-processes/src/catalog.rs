//! 拥有带审计外层事务的目录流程。

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

use crate::Result;
use crate::adapters::catalog_service;
use crate::audit::run_audited;

/// 返回目录流程模块名。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回稳定模块名 `catalog`。
///
/// # 错误
/// 不返回错误。
pub fn process_name() -> &'static str {
    "catalog"
}

/// 创建计量单位，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - 计量单位所在数据库。
/// * `req` - 计量单位创建请求。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回创建后的计量单位视图。
///
/// # 错误
/// 请求校验失败、单位构造失败、审计构造失败，或写入与审计事务失败时返回错误。
pub async fn create_unit_of_measure(
    db: Database,
    req: CreateUnitOfMeasureRequest,
    actor: AuditActor,
) -> Result<UnitOfMeasureView> {
    req.validate()?;
    let id = UnitOfMeasureId::new(next_id());
    let unit = UnitOfMeasure::new(
        id.clone(),
        {
            let data = UnitOfMeasureData::new(req.unit_code, req.name, req.symbol)
                .with_quantity_scale(req.quantity_scale);
            match req.status {
                Some(status) => data.with_status(status),
                None => data,
            }
        },
        actor.id(),
    )?;
    let audit = actor.clone().resource_log("unit_of_measure.create", "unit_of_measure", id.to_string())?;
    let unit_for_tx = unit.clone();
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            db.unit_of_measures().create(&unit_for_tx, executor).await?;
            Ok(())
        })
    })
    .await?;
    Ok(unit.into())
}

/// 创建商品分类，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - 商品分类所在数据库。
/// * `req` - 商品分类创建请求。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回创建后的商品分类视图。
///
/// # 错误
/// 请求校验失败、上级分类链校验失败、分类构造失败，或写入与审计事务失败时返回错误。
pub async fn create_product_category(
    db: Database,
    req: CreateProductCategoryRequest,
    actor: AuditActor,
) -> Result<ProductCategoryView> {
    req.validate()?;
    let parent_id = req.parent_category_id.clone();
    let id = ProductCategoryId::new(next_id());
    catalog_service(db.clone()).ensure_parent_chain_ok(id.as_ref(), parent_id.as_ref()).await?;
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
    let audit = actor.clone().resource_log("product_category.create", "product_category", id.to_string())?;
    let category_for_tx = category.clone();
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            db.product_categories().create(&category_for_tx, executor).await?;
            Ok(())
        })
    })
    .await?;
    Ok(category.into())
}

/// 创建 SKU 属性，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - SKU 属性所在数据库。
/// * `req` - SKU 属性创建请求。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回创建后的 SKU 属性视图。
///
/// # 错误
/// 请求校验失败、属性构造失败，或写入与审计事务失败时返回错误。
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
    let audit = actor.clone().resource_log("sku_attribute.create", "sku_attribute", id.to_string())?;
    let attribute_for_tx = attribute.clone();
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            db.sku_attributes().create(&attribute_for_tx, executor).await?;
            Ok(())
        })
    })
    .await?;
    Ok(attribute.into())
}

/// 创建 SKU 属性值，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - SKU 属性值所在数据库。
/// * `req` - SKU 属性值创建请求。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回创建后的 SKU 属性值视图。
///
/// # 错误
/// 请求校验失败、所属属性加载失败、属性值构造失败，或写入与审计事务失败时返回错误。
pub async fn create_sku_attribute_value(
    db: Database,
    req: CreateSkuAttributeValueRequest,
    actor: AuditActor,
) -> Result<SkuAttributeValueView> {
    req.validate()?;
    catalog_service(db.clone()).load_attribute(req.attribute_id.as_ref()).await?;
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
    let audit =
        actor.clone().resource_log("sku_attribute_value.create", "sku_attribute_value", id.to_string())?;
    let value_for_tx = value.clone();
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            db.sku_attribute_values().create(&value_for_tx, executor).await?;
            Ok(())
        })
    })
    .await?;
    Ok(value.into())
}
