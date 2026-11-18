//! 旧演示记录的恢复读取、生成授权和版本保护写入使用同一审计事务。
use std::sync::Arc;

use application_core::AuditActor;
use entity_core::{HasBaseModel, NOT_DELETED_TIMESTAMP};
use erp_audit::AuditActorLogs;
use erp_audit::execution::run_audited;
use erp_catalog::{CatalogExt, Sku};
use erp_customer::CustomerExt;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_party::PartyExt;
use erp_permissions::demo_master_data;
use erp_supplier::SupplierExt;
use erp_warehouse::WarehouseExt;
use mongodb::Database;
use persistence_core::{Executor, Repository};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Error, Result};

// 用同一写事务校验演示生成权限并恢复登记商品及其 SKU，使原商品关系可继续用于补图和上架。商品缺失、版本冲突或读取失败时拒绝。
pub(super) async fn restore_product_graph(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    product_id: &str,
    sku_ids: &[String],
) -> Result<()> {
    let audit = actor.clone().resource_log("product.restore", "product", product_id.to_string())?;
    let actor = actor.clone();
    let product_id = product_id.to_owned();
    let sku_ids = sku_ids.to_vec();
    run_audited(db, audit, move |db, executor| {
        Box::pin(async move {
            authorization
                .require(&actor, &demo_master_data::APPLY, AccessTarget::Operation, executor)
                .await?;
            restore_product_records(db, &product_id, &sku_ids, executor).await
        })
    })
    .await
}

// 从当前写事务读取商品和 SKU，再由仓储按版本恢复软删除记录，避免使用过期版本覆盖并发修改。
async fn restore_product_records(
    db: &Database,
    product_id: &str,
    sku_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut product = db
        .products()
        .find_by_id_including_deleted(product_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("演示商品已不在库中，无法恢复".into()))?;
    let skus = load_skus(db, product_id, sku_ids, executor).await?;

    if product.base.deleted_at != NOT_DELETED_TIMESTAMP {
        db.products().restore(&mut product, executor).await?;
    }
    for mut sku in skus {
        if sku.base.deleted_at != NOT_DELETED_TIMESTAMP {
            db.skus().restore(&mut sku, executor).await?;
        }
    }
    Ok(())
}

// 合并商品现有关系与历史登记中的 SKU，包含软删除项，供恢复流程检查并恢复原商品关系。
async fn load_skus(
    db: &Database,
    product_id: &str,
    sku_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<Sku>> {
    let mut skus = db
        .skus()
        .find_many_by_field_including_deleted("product_id", product_id.to_string(), executor)
        .await?;

    for sku_id in sku_ids {
        if skus.iter().any(|sku| sku.base.id == *sku_id) {
            continue;
        }
        if let Some(sku) = db.skus().find_by_id_including_deleted(sku_id, executor).await? {
            skus.push(sku);
        }
    }
    Ok(skus)
}

// 按实际主键在同一授权写事务恢复登记仓库，已有效则保留；无生成权限、记录缺失、版本冲突或持久化失败时停止。
pub(super) async fn restore_warehouse(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
) -> Result<()> {
    restore_record(db, authorization, actor, "warehouse", id, Database::warehouses).await
}

// 在授权写事务恢复原客户角色，使销售种子重试沿用原客户 ID；无权、记录缺失、版本冲突或持久化失败时停止。
pub(super) async fn restore_customer(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
) -> Result<()> {
    restore_record(db, authorization, actor, "customer", id, Database::customer_accounts).await
}

// 在同一事务校验生成权限、读取并恢复登记的供应商角色，保留供给引用使用的主键；无权、记录缺失、版本冲突或持久化失败时停止。
pub(super) async fn restore_supplier(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
) -> Result<()> {
    restore_record(db, authorization, actor, "supplier", id, Database::supplier_accounts).await
}

// 在授权写事务按登记主键恢复主体，供客户和供应商继续使用原主体关系；无权、主体缺失、版本冲突或持久化失败时停止。
pub(super) async fn restore_party(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
) -> Result<()> {
    restore_record(db, authorization, actor, "party", id, Database::parties).await
}

// 在原写事务核验权限并按种类恢复已登记品牌、分类或计量单位，保留商品字典引用；无权、记录缺失、版本冲突或持久化失败时停止。
pub(super) async fn restore_dictionary(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    kind: DemoKindDictionary,
    id: &str,
) -> Result<()> {
    match kind {
        DemoKindDictionary::Unit => {
            restore_record(db, authorization, actor, "unit_of_measure", id, Database::unit_of_measures).await
        },
        DemoKindDictionary::Brand => {
            restore_record(db, authorization, actor, "product_brand", id, Database::product_brands).await
        },
        DemoKindDictionary::Category => {
            restore_record(db, authorization, actor, "product_category", id, Database::product_categories)
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

// 在原审计事务读取目标和当前版本，仅对软删除记录使用仓储 CAS 恢复，保证权限、恢复及审计基于同一次写入。
async fn restore_record<T>(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    resource: &str,
    id: &str,
    repository: for<'a> fn(&'a Database) -> Repository<'a, T>,
) -> Result<()>
where
    T: HasBaseModel + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    let audit = actor.clone().resource_log("demo_master_data.restore", resource, id.to_owned())?;
    let id = id.to_owned();
    let actor = actor.clone();
    run_audited(db, audit, move |db, executor| {
        Box::pin(async move {
            authorization
                .require(&actor, &demo_master_data::APPLY, AccessTarget::Operation, executor)
                .await?;

            let repository = repository(db);
            let mut entity = repository
                .find_by_id_including_deleted(&id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("演示记录已不在库中，无法恢复".into()))?;
            if entity.base().deleted_at != NOT_DELETED_TIMESTAMP {
                repository.restore(&mut entity, executor).await?;
            }
            Ok(())
        })
    })
    .await
}
