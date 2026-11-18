//! 选品授权快照：列表、详情重验与写命令复用同一范围。

use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::sales_selection;
use erp_sales::dto::sales_selection::{
    PrepareSalesSelectionRequest, PublishSalesSelectionRequest, SalesSelectionBookletView,
    SalesSelectionSessionView,
};
use erp_sales::entity::sales_selection::{FirstNonEmptyMemberImage, LinkTokenCrypto};
use erp_sales::repository::sales_selection::SalesSelectionDomainRepository;
use erp_sales::service::sales_selection::{SalesSelectionService, SelectionAccess};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use storage::S3Storage;

use super::adapters::{CatalogAdapter, ImageAdapter};
use super::read;
use crate::services::SalesServices;
use crate::{Error, Result};

/// 校验认证创建人及其主属组织在创建动作范围内。
///
/// # 参数
/// * `access` - 选品范围访问器
/// * `actor` - 已认证操作人
/// * `owner` - 拟写入的显式销售负责人
/// * `org` - 拟写入的业务组织
/// * `executor` - 调用方执行器
///
/// # 返回
/// 范围允许时成功。
///
/// # 错误
/// 无动作权限或责任不在范围内时拒绝。
pub(crate) async fn check_create_scope(
    access: &SelectionAccess,
    actor: &AuditActor,
    customer_id: &str,
    owner: &str,
    org: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if owner.trim().is_empty() || org.trim().is_empty() {
        return Err(Error::ValidationError("销售负责人与业务组织必填".into()));
    }
    access.require_new(actor, customer_id, owner, org, executor).await?;
    Ok(())
}

/// 启动准备，并在实际写事务内重验统一授权。
///
/// # 参数
/// * `db` - 选品数据库
/// * `storage` - 对象存储
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `req` - 准备命令
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回准备中详情。
///
/// # 错误
/// 无权操作、状态或版本非法时拒绝。
pub(crate) async fn start_prepare_checked(
    db: &Database,
    storage: &S3Storage,
    services: &SalesServices,
    access: SelectionAccess,
    id: String,
    req: PrepareSalesSelectionRequest,
    actor: AuditActor,
) -> Result<SalesSelectionBookletView> {
    let mut command = req;
    command.booklet_id = id;
    let catalog = CatalogAdapter { query: services.catalog.clone() };
    let images = ImageAdapter { files: services.files.clone(), storage: Arc::new(storage.clone()) };
    let owned = db.clone();
    Ok(SalesSelectionService::new(owned, access.authorization())
        .start_prepare(command, &actor, &catalog, &images, &FirstNonEmptyMemberImage)
        .await?)
}

/// 授权后发布。
///
/// # 参数
/// * `db` - 选品数据库
/// * `storage` - 对象存储
/// * `crypto` - 链接编解码器
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `req` - 发布命令
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回含链接的详情。
///
/// # 错误
/// 无权操作、复核失败或状态不允许时拒绝。
pub(crate) async fn publish_checked(
    db: &Database,
    services: &SalesServices,
    crypto: &LinkTokenCrypto,
    access: SelectionAccess,
    id: String,
    req: PublishSalesSelectionRequest,
    actor: AuditActor,
) -> Result<SalesSelectionBookletView> {
    let catalog = Arc::new(CatalogAdapter { query: services.catalog.clone() });
    Ok(SalesSelectionService::new(db.clone(), access.authorization())
        .publish(&id, req, &actor, catalog, crypto)
        .await?)
}

/// 授权后复制链接详情。
///
/// # 参数
/// * `db` - 选品数据库
/// * `crypto` - 链接编解码器
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回含路径的详情。
///
/// # 错误
/// 无链接或无权操作时拒绝。
pub(crate) async fn copy_link_checked(
    db: &Database,
    crypto: &LinkTokenCrypto,
    access: SelectionAccess,
    id: &str,
    actor: &AuditActor,
) -> Result<SalesSelectionBookletView> {
    let owned = db.clone();
    let authorization = access.authorization();
    let id = id.to_owned();
    let actor = actor.clone();
    let crypto = crypto.clone();
    db.client()
        .with_transaction(move |executor| {
            Box::pin(async move {
                read::checked(
                    &access,
                    &actor,
                    &sales_selection::BOOKLET_COPY_LINK,
                    executor,
                    move |scope, executor| {
                        Box::pin(async move {
                            let book = SalesSelectionDomainRepository::new(&owned)
                                .find_authorized_booklet(&id, scope, executor)
                                .await?
                                .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))?;
                            let service = SalesSelectionService::new(owned, authorization);
                            let token = service.copy_link(&id, &crypto, executor).await?;
                            let mut view = service.detail_view(&book, None, executor).await?;
                            let path = format!("/s/{token}");
                            view.public_path = Some(path.clone());
                            view.public_url = Some(path);
                            Ok(view)
                        })
                    },
                )
                .await
            })
        })
        .await
}

/// 授权后读取内部会话。
///
/// 授权和业务资料使用同一快照执行器。
///
/// # 参数
/// * `db` - 选品数据库
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回当前会话。
///
/// # 错误
/// 无会话或无权查看时拒绝。
pub(crate) async fn session_checked(
    db: &Database,
    access: SelectionAccess,
    id: &str,
    actor: &AuditActor,
) -> Result<SalesSelectionSessionView> {
    let owned = db.clone();
    let authorization = access.authorization();
    let id = id.to_owned();
    let actor = actor.clone();
    db.client()
        .with_transaction(move |executor| {
            Box::pin(async move {
                read::checked(
                    &access,
                    &actor,
                    &sales_selection::BOOKLET_DETAIL,
                    executor,
                    move |scope, executor| {
                        Box::pin(async move {
                            let book = SalesSelectionDomainRepository::new(&owned)
                                .find_authorized_booklet(&id, scope, executor)
                                .await?
                                .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))?;
                            Ok(SalesSelectionService::new(owned, authorization)
                                .session_of(&book.base.id, executor)
                                .await?)
                        })
                    },
                )
                .await
            })
        })
        .await
}

/// 授权后读取册图片对象键；同一执行器内重验。
///
/// 授权和业务资料使用同一快照执行器。
///
/// # 参数
/// * `db` - 选品数据库
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `asset_id` - 资产
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回快照对象键。
///
/// # 错误
/// 资产不属于本册或无权查看时拒绝。
pub(crate) async fn image_key_checked(
    db: &Database,
    access: SelectionAccess,
    id: &str,
    asset_id: &str,
    actor: &AuditActor,
) -> Result<String> {
    let owned = db.clone();
    let authorization = access.authorization();
    let id = id.to_owned();
    let asset_id = asset_id.to_owned();
    let actor = actor.clone();
    db.client()
        .with_transaction(move |executor| {
            Box::pin(async move {
                read::checked(
                    &access,
                    &actor,
                    &sales_selection::BOOKLET_DETAIL,
                    executor,
                    move |scope, executor| {
                        Box::pin(async move {
                            let book = SalesSelectionDomainRepository::new(&owned)
                                .find_authorized_booklet(&id, scope, executor)
                                .await?
                                .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))?;
                            Ok(SalesSelectionService::new(owned, authorization)
                                .image_key_of(&book.base.id, &asset_id, executor)
                                .await?)
                        })
                    },
                )
                .await
            })
        })
        .await
}
