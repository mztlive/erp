//! 在同一业务事务中登记上传文件的目录命令。

use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::{
    CatalogService, CreateProductBrandRequest, CreateProductRequest, PendingAttachmentBatch,
    ProductBrandView, ProductView, UpdateProductBrandRequest, UpdateProductRequest,
};
use erp_file_assets::PendingFileAssets;
use erp_support::PendingFileAssetRequest;

use crate::Result;

/// 创建商品品牌，并在同一事务中持久化已上传的标志。
///
/// # 参数
/// * `service` - 已装配审计与文件资产 Port 的商品服务。
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
    service: CatalogService,
    req: CreateProductBrandRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductBrandView> {
    let pending = prepare_attachments(asset_requests, &actor)?;
    service.product_brand_create_with_assets(req, pending, &actor).await.map_err(Into::into)
}

/// 更新商品品牌，并在同一事务中持久化已上传的标志。
///
/// # 参数
/// * `service` - 已装配审计与文件资产 Port 的商品服务。
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
    service: CatalogService,
    id: String,
    req: UpdateProductBrandRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductBrandView> {
    let pending = prepare_attachments(asset_requests, &actor)?;
    service.product_brand_update_with_assets(&id, req, pending, &actor).await.map_err(Into::into)
}

/// 创建商品，并在同一事务中持久化已上传的媒体。
///
/// # 参数
/// * `service` - 已装配当前数据范围、审计与文件资产 Port 的商品服务。
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
    service: CatalogService,
    req: CreateProductRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductView> {
    let pending = prepare_attachments(asset_requests, &actor)?;
    service.product_create_with_assets(req, pending, &actor).await.map_err(Into::into)
}

/// 更新商品，并在同一事务中持久化已上传的媒体。
///
/// # 参数
/// * `service` - 已装配当前数据范围、审计与文件资产 Port 的商品服务。
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
    service: CatalogService,
    id: String,
    req: UpdateProductRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductView> {
    let pending = prepare_attachments(asset_requests, &actor)?;
    service.product_update_with_assets(&id, req, pending, &actor).await.map_err(Into::into)
}

fn prepare_attachments(
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: &AuditActor,
) -> Result<Arc<dyn PendingAttachmentBatch>> {
    Ok(PendingFileAssets::prepare(asset_requests, actor)?.into_catalog_batch())
}
