//! 拥有带审计外层事务的目录流程。

use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::run_audited;
use erp_catalog::entity::catalog::product_category::{ProductCategory, ProductCategoryData};
use erp_catalog::entity::catalog::sku_attribute::{SkuAttribute, SkuAttributeData};
use erp_catalog::entity::catalog::sku_attribute_value::{SkuAttributeValue, SkuAttributeValueData};
use erp_catalog::entity::catalog::unit_of_measure::{UnitOfMeasure, UnitOfMeasureData};
use erp_catalog::entity::catalog::{
    EnableStatus, ProductCategoryId, SkuAttributeId, SkuAttributeValueId, UnitOfMeasureId,
};
use erp_catalog::{
    CatalogExt, CatalogService, CreateProductCategoryRequest, CreateSkuAttributeRequest,
    CreateSkuAttributeValueRequest, CreateUnitOfMeasureRequest, ProductCategoryView, SkuAttributeValueView,
    SkuAttributeView, UnitOfMeasureView,
};
use erp_core::common::time::BusinessDate;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::catalog;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind, next_id};
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use crate::{Error, Result};

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
/// * `authorization` - 应用共享授权服务。
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
    authorization: Arc<AuthorizationService>,
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
        Box::pin(async move { persist_unit(db, &authorization, &actor, &unit_for_tx, executor).await })
    })
    .await?;
    Ok(unit.into())
}

/// 在原审计事务完成创建资格检查和计量单位写入。
async fn persist_unit(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    unit: &UnitOfMeasure,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization.require(actor, &catalog::UNIT_OF_MEASURE_CREATE, AccessTarget::Operation, executor).await?;
    db.unit_of_measures().create(unit, executor).await?;
    Ok(())
}

/// 创建商品分类，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - 商品分类所在数据库。
/// * `authorization` - 应用共享授权服务。
/// * `service` - 应用装配的商品领域服务，供原有资格读取使用。
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
    authorization: Arc<AuthorizationService>,
    service: CatalogService,
    req: CreateProductCategoryRequest,
    actor: AuditActor,
) -> Result<ProductCategoryView> {
    req.validate()?;
    let id = ProductCategoryId::new(next_id());
    let audit = actor.clone().resource_log("product_category.create", "product_category", id.to_string())?;
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            let category = persist_category(db, &authorization, &service, id, req, &actor, executor).await?;
            Ok(category.into())
        })
    })
    .await
}

/// 创建资格先于取号；父链检查和分类写入沿用原审计事务。
async fn persist_category(
    db: &Database,
    authorization: &AuthorizationService,
    service: &CatalogService,
    id: ProductCategoryId,
    mut req: CreateProductCategoryRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ProductCategory> {
    authorization
        .require(actor, &catalog::PRODUCT_CATEGORY_CREATE, AccessTarget::Operation, executor)
        .await?;
    service.ensure_parent_chain_ok(id.as_ref(), req.parent_category_id.as_ref(), executor).await?;
    if req.category_code.trim().is_empty() {
        req.category_code = DocumentNumberGenerator::new(db.clone())
            .next_number(DocumentNumberKind::ProductCategory, BusinessDate::today().as_naive_date(), executor)
            .await
            .map_err(|error| Error::Internal(error.to_string()))?;
    }
    let category = ProductCategory::new(
        id,
        ProductCategoryData {
            category_code: req.category_code,
            parent_category_id: req.parent_category_id,
            name: req.name,
            product_kind: req.product_kind,
            status: req.status.unwrap_or(EnableStatus::Active),
        },
        actor.id(),
    )?;
    db.product_categories().create(&category, executor).await?;
    Ok(category)
}

/// 创建 SKU 属性，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - SKU 属性所在数据库。
/// * `authorization` - 应用共享授权服务。
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
    authorization: Arc<AuthorizationService>,
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
            authorization
                .require(&actor, &catalog::SKU_ATTRIBUTE_CREATE, AccessTarget::Operation, executor)
                .await?;
            db.sku_attributes().create(&attribute_for_tx, executor).await?;
            Ok::<(), Error>(())
        })
    })
    .await?;
    Ok(attribute.into())
}

/// 创建 SKU 属性值，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - SKU 属性值所在数据库。
/// * `authorization` - 应用共享授权服务。
/// * `service` - 应用装配的商品领域服务，供原有资格读取使用。
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
    authorization: Arc<AuthorizationService>,
    service: CatalogService,
    req: CreateSkuAttributeValueRequest,
    actor: AuditActor,
) -> Result<SkuAttributeValueView> {
    req.validate()?;
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
            authorization
                .require(&actor, &catalog::SKU_ATTRIBUTE_VALUE_CREATE, AccessTarget::Operation, executor)
                .await?;
            service.load_attribute(value_for_tx.attribute_id.as_ref(), executor).await?;
            db.sku_attribute_values().create(&value_for_tx, executor).await?;
            Ok::<(), Error>(())
        })
    })
    .await?;
    Ok(value.into())
}

#[cfg(test)]
mod tests {
    use erp_catalog::CatalogAccess;
    use erp_catalog::entity::catalog::ProductKind;
    use erp_catalog::ports::{EmptyFileAssetFacts, FailClosedAuditPort};
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn category_creation_rejects_authorization_failure_before_parent_reads_or_number_allocation() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("category_guard");
        let authorization = unavailable_authorization();
        let service = CatalogService::new(
            db.clone(),
            Arc::new(FailClosedAuditPort),
            Arc::new(EmptyFileAssetFacts),
            CatalogAccess::new(db.clone(), authorization.clone()),
        );
        let actor = AuditActor::new("operator".into(), "operator".into(), AccountKind::Admin);
        let request = CreateProductCategoryRequest {
            category_code: String::new(),
            parent_category_id: Some(ProductCategoryId::new("parent")),
            name: "分类".into(),
            product_kind: ProductKind::Physical,
            status: None,
        };
        let error = persist_category(
            &db,
            &authorization,
            &service,
            ProductCategoryId::new("category"),
            request,
            &actor,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn unit_creation_rejects_authorization_failure_before_writing() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("catalog_guard");
        let actor = AuditActor::new("operator".into(), "operator".into(), AccountKind::Admin);
        let unit = UnitOfMeasure::new(
            UnitOfMeasureId::new("unit"),
            UnitOfMeasureData::new("EA", "件", "件"),
            actor.id(),
        )
        .unwrap();
        let error =
            persist_unit(&db, &unavailable_authorization(), &actor, &unit, &mut SnapshotExecutor::default())
                .await
                .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
