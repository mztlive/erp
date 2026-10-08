//! 兼容恢复旧版本软删除的演示记录；演示清理统一执行硬删除。

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

/// 恢复商品及其 SKU。商品记录不存在时返回未找到，调用方改为重新创建。
///
/// # 参数
/// * `db` - 目标数据库
/// * `actor` - 当前操作人
/// * `product_id` - 商品 ID
/// * `sku_ids` - 创建时记下的 SKU ID
///
/// # 返回
/// 商品及其记下的 SKU 已是有效记录，或已在同一审计事务中恢复时无返回值。
///
/// # 错误
/// 商品不存在时返回 `NotFound`。审计构造、SKU 读取或恢复写入失败时返回对应错误。
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

/// 恢复仓库。记录不存在时返回未找到。
///
/// # 参数
/// * `db` - 目标数据库。
/// * `actor` - 当前操作人，用于恢复审计。
/// * `id` - 仓库 ID。
///
/// # 返回
/// 仓库已有效或已恢复时无返回值。
///
/// # 错误
/// 仓库不存在时返回 `NotFound`。审计构造或恢复写入失败时返回对应错误。
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
///
/// # 参数
/// * `db` - 目标数据库。
/// * `actor` - 当前操作人，用于恢复审计。
/// * `id` - 客户角色 ID。
///
/// # 返回
/// 客户已有效或已恢复时无返回值。
///
/// # 错误
/// 客户不存在时返回 `NotFound`。审计构造或恢复写入失败时返回对应错误。
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
///
/// # 参数
/// * `db` - 目标数据库。
/// * `actor` - 当前操作人，用于恢复审计。
/// * `id` - 供应商角色 ID。
///
/// # 返回
/// 供应商已有效或已恢复时无返回值。
///
/// # 错误
/// 供应商不存在时返回 `NotFound`。审计构造或恢复写入失败时返回对应错误。
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
///
/// # 参数
/// * `db` - 目标数据库。
/// * `actor` - 当前操作人，用于恢复审计。
/// * `id` - 主体 ID。
///
/// # 返回
/// 主体已有效或已恢复时无返回值。
///
/// # 错误
/// 主体不存在时返回 `NotFound`。审计构造或恢复写入失败时返回对应错误。
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
///
/// # 参数
/// * `db` - 目标数据库。
/// * `actor` - 当前操作人，用于恢复审计。
/// * `kind` - 要恢复的字典种类。
/// * `id` - 字典记录 ID。
///
/// # 返回
/// 记录已有效或已恢复时无返回值。
///
/// # 错误
/// 记录不存在时返回 `NotFound`。审计构造或恢复写入失败时返回对应错误。
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

/// 仅恢复旧版本软删除记录，审计与恢复在同一事务完成。
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
    if !entity.is_deleted() {
        return Ok(());
    }
    let action = "demo_master_data.restore";
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
