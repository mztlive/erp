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

/// 恢复商品及登记的 SKU，使用同一快照核验当前版本。
/// # 参数
/// db 为数据库，authorization 为共享授权服务；actor 为操作人，product_id、sku_ids 为实际登记身份。
/// # 返回
/// 商品和登记 SKU 已有效或已恢复。
/// # 错误
/// 缺少演示生成权限、商品不存在、CAS 版本冲突或数据库故障时拒绝。
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

/// 恢复决定所用的商品与 SKU 均从当前写事务读取；通用仓储使用版本 CAS。
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

/// 收集该商品当前和历史登记 SKU，不提前丢弃已软删除的记录。
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

/// 恢复仓库，读取、授权和写入使用同一事务。
/// # 参数
/// db、authorization 为共享设施；actor 为当前账号，id 为登记的仓库身份。
/// # 返回
/// 仓库已有效或已恢复。
/// # 错误
/// 无生成资格、仓库不存在、版本冲突或数据库故障时拒绝。
pub(super) async fn restore_warehouse(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
) -> Result<()> {
    restore_record(db, authorization, actor, "warehouse", id, Database::warehouses).await
}

/// 恢复客户角色，读取、授权和写入使用同一事务。
/// # 参数
/// db、authorization 为共享设施；actor 为当前账号，id 为登记的客户身份。
/// # 返回
/// 客户已有效或已恢复。
/// # 错误
/// 无生成资格、客户不存在、版本冲突或数据库故障时拒绝。
pub(super) async fn restore_customer(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
) -> Result<()> {
    restore_record(db, authorization, actor, "customer", id, Database::customer_accounts).await
}

/// 恢复供应商角色，读取、授权和写入使用同一事务。
/// # 参数
/// db、authorization 为共享设施；actor 为当前账号，id 为登记的供应商身份。
/// # 返回
/// 供应商已有效或已恢复。
/// # 错误
/// 无生成资格、供应商不存在、版本冲突或数据库故障时拒绝。
pub(super) async fn restore_supplier(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
) -> Result<()> {
    restore_record(db, authorization, actor, "supplier", id, Database::supplier_accounts).await
}

/// 恢复主体，读取、授权和写入使用同一事务。
/// # 参数
/// db、authorization 为共享设施；actor 为当前账号，id 为登记的主体身份。
/// # 返回
/// 主体已有效或已恢复。
/// # 错误
/// 无生成资格、主体不存在、版本冲突或数据库故障时拒绝。
pub(super) async fn restore_party(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
) -> Result<()> {
    restore_record(db, authorization, actor, "party", id, Database::parties).await
}

/// 恢复品牌、分类或计量单位，授权与目标读取沿用实际写事务。
/// # 参数
/// db、authorization 为共享设施；actor 为当前账号，kind、id 为登记种类及身份。
/// # 返回
/// 字典记录已有效或已恢复。
/// # 错误
/// 无生成资格、字典不存在、版本冲突或数据库故障时拒绝。
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

/// 在原审计事务取得当前实体并恢复，写入版本由 Repository 的 CAS 保护。
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
