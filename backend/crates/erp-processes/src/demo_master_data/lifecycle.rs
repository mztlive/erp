//! 恢复或软删除演示记录本身。业务删除仍走各自的领域入口。

use application_core::AuditActor;
use entity_core::{BaseModel, NOT_DELETED_TIMESTAMP};
use erp_audit::AuditActorLogs;
use erp_catalog::{CatalogExt, Product, Sku};
use erp_customer::{CustomerAccount, CustomerExt};
use erp_party::{Party, PartyExt};
use erp_supplier::{SupplierAccount, SupplierExt};
use erp_warehouse::{Warehouse, WarehouseExt};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::audit::run_audited;
use crate::{Error, Result};

fn is_active(base: &BaseModel) -> bool {
    base.deleted_at == NOT_DELETED_TIMESTAMP
}

/// 软删除商品及其 SKU。记录已经不在时视为已删除。
///
/// # 参数
/// * `db` - 目标数据库
/// * `actor` - 当前操作人
/// * `product_id` - 商品 ID
/// * `sku_ids` - 创建时记下的 SKU ID
///
/// # 错误
/// 写入失败时返回错误。
pub(super) async fn delete_product_graph(
    db: &Database,
    actor: &AuditActor,
    product_id: &str,
    sku_ids: &[String],
) -> Result<()> {
    let product = db.products().find_by_id_including_deleted(product_id, &mut NoTransaction).await?;
    let mut skus = load_skus(db, product_id, sku_ids).await?;
    let product_active = product.as_ref().is_some_and(|item| is_active(&item.base));
    let sku_active = skus.iter().any(|sku| is_active(&sku.base));
    if !product_active && !sku_active {
        return Ok(());
    }
    let audit = actor.clone().resource_log("product.delete", "product", product_id.to_string())?;
    run_audited(db, audit, move |db, executor| {
        Box::pin(async move {
            if let Some(mut product) = product
                && is_active(&product.base)
            {
                db.products().soft_delete(&mut product, executor).await?;
            }
            for mut sku in skus.drain(..) {
                if is_active(&sku.base) {
                    db.skus().soft_delete(&mut sku, executor).await?;
                }
            }
            Ok(())
        })
    })
    .await
}

/// 恢复商品及其 SKU。商品记录不存在时返回未找到，调用方改为重新创建。
///
/// # 参数
/// * `db` - 目标数据库
/// * `actor` - 当前操作人
/// * `product_id` - 商品 ID
/// * `sku_ids` - 创建时记下的 SKU ID
///
/// # 错误
/// 商品不存在或写入失败时返回错误。
pub(super) async fn restore_product_graph(
    db: &Database,
    actor: &AuditActor,
    product_id: &str,
    sku_ids: &[String],
) -> Result<()> {
    let product = db
        .products()
        .find_by_id_including_deleted(product_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("演示商品已不在库中，无法恢复".to_string()))?;
    let mut skus = load_skus(db, product_id, sku_ids).await?;
    if is_active(&product.base) && skus.iter().all(|sku| is_active(&sku.base)) {
        return Ok(());
    }
    let audit = actor.clone().resource_log("product.restore", "product", product_id.to_string())?;
    run_audited(db, audit, move |db, executor| {
        Box::pin(async move {
            let mut product = product;
            if !is_active(&product.base) {
                db.products().restore(&mut product, executor).await?;
            }
            for mut sku in skus.drain(..) {
                if !is_active(&sku.base) {
                    db.skus().restore(&mut sku, executor).await?;
                }
            }
            Ok(())
        })
    })
    .await
}

async fn load_skus(db: &Database, product_id: &str, sku_ids: &[String]) -> Result<Vec<Sku>> {
    let mut skus = db
        .skus()
        .find_many_by_field_including_deleted("product_id", product_id.to_string(), &mut NoTransaction)
        .await?;
    for sku_id in sku_ids {
        if skus.iter().any(|sku| sku.base.id == *sku_id) {
            continue;
        }
        if let Some(sku) = db.skus().find_by_id_including_deleted(sku_id, &mut NoTransaction).await? {
            skus.push(sku);
        }
    }
    Ok(skus)
}

/// 软删除仓库。记录已经不在时视为已删除。
pub(super) async fn delete_warehouse(db: &Database, actor: &AuditActor, id: &str) -> Result<()> {
    delete_loaded(db, actor, "warehouse", load_warehouse(db, id).await?, |db, executor, entity| {
        Box::pin(async move {
            db.warehouses().soft_delete(entity, executor).await?;
            Ok(())
        })
    })
    .await
}

/// 恢复仓库。记录不存在时返回未找到。
pub(super) async fn restore_warehouse(db: &Database, actor: &AuditActor, id: &str) -> Result<()> {
    let entity = load_warehouse(db, id)
        .await?
        .ok_or_else(|| Error::NotFound("演示仓库已不在库中，无法恢复".to_string()))?;
    restore_loaded(db, actor, "warehouse", entity, |db, executor, entity| {
        Box::pin(async move {
            db.warehouses().restore(entity, executor).await?;
            Ok(())
        })
    })
    .await
}

async fn load_warehouse(db: &Database, id: &str) -> Result<Option<Warehouse>> {
    Ok(db.warehouses().find_by_id_including_deleted(id, &mut NoTransaction).await?)
}

/// 恢复客户角色。记录不存在时返回未找到。
pub(super) async fn restore_customer(db: &Database, actor: &AuditActor, id: &str) -> Result<()> {
    let entity = db
        .customer_accounts()
        .find_by_id_including_deleted(id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("演示客户已不在库中，无法恢复".to_string()))?;
    restore_loaded(db, actor, "customer", entity, |db, executor, entity| {
        Box::pin(async move {
            db.customer_accounts().restore(entity, executor).await?;
            Ok(())
        })
    })
    .await
}

/// 恢复供应商角色。记录不存在时返回未找到。
pub(super) async fn restore_supplier(db: &Database, actor: &AuditActor, id: &str) -> Result<()> {
    let entity = db
        .supplier_accounts()
        .find_by_id_including_deleted(id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("演示供应商已不在库中，无法恢复".to_string()))?;
    restore_loaded(db, actor, "supplier", entity, |db, executor, entity| {
        Box::pin(async move {
            db.supplier_accounts().restore(entity, executor).await?;
            Ok(())
        })
    })
    .await
}

/// 恢复主体。记录不存在时返回未找到。
pub(super) async fn restore_party(db: &Database, actor: &AuditActor, id: &str) -> Result<()> {
    let entity = db
        .parties()
        .find_by_id_including_deleted(id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("演示主体已不在库中，无法恢复".to_string()))?;
    restore_loaded(db, actor, "party", entity, |db, executor, entity| {
        Box::pin(async move {
            db.parties().restore(entity, executor).await?;
            Ok(())
        })
    })
    .await
}

/// 恢复品牌、分类或计量单位。
pub(super) async fn restore_dictionary(
    db: &Database,
    actor: &AuditActor,
    kind: DemoKindDictionary,
    id: &str,
) -> Result<()> {
    match kind {
        DemoKindDictionary::Unit => {
            let entity = required_unit(db, id).await?;
            restore_loaded(db, actor, "unit_of_measure", entity, |db, executor, entity| {
                Box::pin(async move {
                    db.unit_of_measures().restore(entity, executor).await?;
                    Ok(())
                })
            })
            .await
        },
        DemoKindDictionary::Brand => {
            let entity = required_brand(db, id).await?;
            restore_loaded(db, actor, "product_brand", entity, |db, executor, entity| {
                Box::pin(async move {
                    db.product_brands().restore(entity, executor).await?;
                    Ok(())
                })
            })
            .await
        },
        DemoKindDictionary::Category => {
            let entity = required_category(db, id).await?;
            restore_loaded(db, actor, "product_category", entity, |db, executor, entity| {
                Box::pin(async move {
                    db.product_categories().restore(entity, executor).await?;
                    Ok(())
                })
            })
            .await
        },
    }
}

/// 字典恢复时区分三种集合。
#[derive(Clone, Copy)]
pub(super) enum DemoKindDictionary {
    /// 计量单位。
    Unit,
    /// 品牌。
    Brand,
    /// 分类。
    Category,
}

async fn required_unit(db: &Database, id: &str) -> Result<erp_catalog::UnitOfMeasure> {
    db.unit_of_measures()
        .find_by_id_including_deleted(id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("演示计量单位已不在库中，无法恢复".to_string()))
}

async fn required_brand(db: &Database, id: &str) -> Result<erp_catalog::ProductBrand> {
    db.product_brands()
        .find_by_id_including_deleted(id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("演示品牌已不在库中，无法恢复".to_string()))
}

async fn required_category(db: &Database, id: &str) -> Result<erp_catalog::ProductCategory> {
    db.product_categories()
        .find_by_id_including_deleted(id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("演示分类已不在库中，无法恢复".to_string()))
}

async fn delete_loaded<T, F>(
    db: &Database,
    actor: &AuditActor,
    resource: &str,
    entity: Option<T>,
    write: F,
) -> Result<()>
where
    T: HasDeleted + Send + 'static,
    F: for<'a> FnOnce(
            &'a Database,
            &'a mut dyn persistence_core::Executor,
            &'a mut T,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>
        + Send
        + 'static,
{
    let Some(entity) = entity else {
        return Ok(());
    };
    set_presence(db, actor, resource, entity, true, write).await
}

async fn restore_loaded<T, F>(
    db: &Database,
    actor: &AuditActor,
    resource: &str,
    entity: T,
    write: F,
) -> Result<()>
where
    T: HasDeleted + Send + 'static,
    F: for<'a> FnOnce(
            &'a Database,
            &'a mut dyn persistence_core::Executor,
            &'a mut T,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>
        + Send
        + 'static,
{
    set_presence(db, actor, resource, entity, false, write).await
}

async fn set_presence<T, F>(
    db: &Database,
    actor: &AuditActor,
    resource: &str,
    entity: T,
    delete: bool,
    write: F,
) -> Result<()>
where
    T: HasDeleted + Send + 'static,
    F: for<'a> FnOnce(
            &'a Database,
            &'a mut dyn persistence_core::Executor,
            &'a mut T,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>
        + Send
        + 'static,
{
    if entity.is_deleted() == delete {
        return Ok(());
    }
    let action = if delete { "demo_master_data.delete" } else { "demo_master_data.restore" };
    let audit = actor.clone().resource_log(action, resource, entity.identity())?;
    run_audited(db, audit, move |db, executor| {
        Box::pin(async move {
            let mut entity = entity;
            write(db, executor, &mut entity).await
        })
    })
    .await
}

trait HasDeleted: Send {
    fn is_deleted(&self) -> bool;
    fn identity(&self) -> String;
}

macro_rules! deleted_identity {
    ($ty:ty) => {
        impl HasDeleted for $ty {
            fn is_deleted(&self) -> bool {
                !is_active(&self.base)
            }
            fn identity(&self) -> String {
                self.base.id.clone()
            }
        }
    };
}

deleted_identity!(Product);
deleted_identity!(Warehouse);
deleted_identity!(CustomerAccount);
deleted_identity!(SupplierAccount);
deleted_identity!(Party);
deleted_identity!(erp_catalog::UnitOfMeasure);
deleted_identity!(erp_catalog::ProductBrand);
deleted_identity!(erp_catalog::ProductCategory);
