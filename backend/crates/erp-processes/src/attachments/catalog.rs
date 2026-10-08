//! 在同一业务事务中登记上传文件的目录命令。

use application_core::AuditActor;
use erp_catalog::{
    CreateProductBrandRequest, CreateProductRequest, ProductBrandView, ProductView,
    UpdateProductBrandRequest, UpdateProductRequest,
};
use erp_identity::SharedRbacService;
use erp_support::PendingFileAssetRequest;
use mongodb::Database;

use super::pending::PendingFileAssets;
use crate::Result;
use crate::adapters::{CatalogPendingAttachments, catalog_service, scoped_catalog_service};

/// 创建商品品牌，并在同一事务中持久化已上传的标志。
///
/// # 参数
/// * `db` - 品牌与文件资产所在数据库。
/// * `req` - 品牌创建请求。
/// * `asset_requests` - 已写入对象存储的标志登记请求；可以为空。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回创建后的品牌视图。
///
/// # 错误
/// 临时文件校验失败，或品牌创建与附件事务失败时返回错误。
pub async fn product_brand_create_with_assets(
    db: Database,
    req: CreateProductBrandRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductBrandView> {
    let pending =
        CatalogPendingAttachments::from_support(PendingFileAssets::prepare(asset_requests, &actor)?.shared());
    catalog_service(db).product_brand_create_with_assets(req, pending, &actor).await.map_err(Into::into)
}

/// 更新商品品牌，并在同一事务中持久化已上传的标志。
///
/// # 参数
/// * `db` - 品牌与文件资产所在数据库。
/// * `id` - 品牌主键。
/// * `req` - 品牌更新请求。
/// * `asset_requests` - 已写入对象存储的标志登记请求；可以为空。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回更新后的品牌视图。
///
/// # 错误
/// 临时文件校验失败，或品牌更新与附件事务失败时返回错误。
pub async fn product_brand_update_with_assets(
    db: Database,
    id: String,
    req: UpdateProductBrandRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductBrandView> {
    let pending =
        CatalogPendingAttachments::from_support(PendingFileAssets::prepare(asset_requests, &actor)?.shared());
    catalog_service(db).product_brand_update_with_assets(&id, req, pending, &actor).await.map_err(Into::into)
}

/// 创建商品，并在同一事务中持久化已上传的媒体。
///
/// # 参数
/// * `db` - 商品与文件资产所在数据库。
/// * `rbac` - 商品范围重验使用的 RBAC 快照。
/// * `req` - 商品创建请求。
/// * `asset_requests` - 已写入对象存储的媒体登记请求。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回创建后的商品视图。
///
/// # 错误
/// 临时文件校验失败，或商品创建与附件事务失败时返回错误。
pub async fn product_create_with_assets(
    db: Database,
    rbac: SharedRbacService,
    req: CreateProductRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductView> {
    let pending =
        CatalogPendingAttachments::from_support(PendingFileAssets::prepare(asset_requests, &actor)?.shared());
    scoped_catalog_service(db, rbac)
        .product_create_with_assets(req, pending, &actor)
        .await
        .map_err(Into::into)
}

/// 更新商品，并在同一事务中持久化已上传的媒体。
///
/// # 参数
/// * `db` - 商品与文件资产所在数据库。
/// * `rbac` - 商品范围重验使用的 RBAC 快照。
/// * `id` - 商品主键。
/// * `req` - 商品更新请求。
/// * `asset_requests` - 已写入对象存储的媒体登记请求。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回更新后的商品视图。
///
/// # 错误
/// 临时文件校验失败，或商品更新与附件事务失败时返回错误。
pub async fn product_update_with_assets(
    db: Database,
    rbac: SharedRbacService,
    id: String,
    req: UpdateProductRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductView> {
    let pending =
        CatalogPendingAttachments::from_support(PendingFileAssets::prepare(asset_requests, &actor)?.shared());
    scoped_catalog_service(db, rbac)
        .product_update_with_assets(&id, req, pending, &actor)
        .await
        .map_err(Into::into)
}
