//! 定向商品补查与目录消息共用的授权和连接复核。
use application_core::AuditActor;
use erp_identity::{AccessControlExt, AccessTarget, AuthorizationService};
use erp_permissions::supplier_api;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_support::BulkJobExt;
use erp_support::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use crate::{Error, Result};

/// 沿用本连接最近一次明确采集授权，拒绝失效账号。
/// # 参数
/// 数据库和服务端连接身份。
/// # 返回
/// 有效采集授权人的审计身份。
/// # 错误
/// 尚未采集、账号缺失或停用时拒绝。
pub(crate) async fn catalog_refresh_actor(db: &Database, connection: &str) -> Result<AuditActor> {
    let history = db.background_jobs().recent_catalog_jobs(connection, &mut NoTransaction).await?;
    let job = history.first().ok_or_else(|| Error::Forbidden("商品补查需要先取得目录采集授权".into()))?;
    let account = db
        .accounts()
        .find_by_id(&job.requested_by, &mut NoTransaction)
        .await?
        .filter(|account| account.is_active_backoffice())
        .ok_or_else(|| Error::Forbidden("商品采集授权人已失效，请重新授权同步".into()))?;
    Ok(AuditActor::new(account.base.id.clone(), account.secret.account().into(), account.kind)
        .with_actor_name_snapshot(Some(account.name))?)
}

/// 在来源应用事务内复核连接与授权人，并与配置写入产生版本冲突保护。
/// # 参数
/// 数据库、补查使用的连接快照、授权人、共享授权服务和同一事务执行器。
/// # 返回
/// 当前配置与账号仍然有效时成功。
/// # 错误
/// 配置版本、供应商或账号变化时拒绝应用。
pub(crate) async fn guard_catalog_refresh(
    db: &Database,
    connection: &SupplierApiConnection,
    actor: &AuditActor,
    authorization: &AuthorizationService,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization.require(actor, &supplier_api::ADOPTIONS_PREVIEW, AccessTarget::Operation, executor).await?;
    let service = SupplierApiService::new(db.clone());
    let mut current = service.load_connection(&connection.base.id, executor).await?;
    if current.technical_config_version != connection.technical_config_version
        || current.supplier_id != connection.supplier_id
        || current.connector_type != connection.connector_type
    {
        return Err(Error::ConflictError("商品补查期间连接配置已变化".into()));
    }
    service.persist_connection(&mut current, executor).await?;
    Ok(())
}
