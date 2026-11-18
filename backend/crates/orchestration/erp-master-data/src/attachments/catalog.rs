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

/// 新增品牌时，把已上传且校验通过的可选标志交给已装配审计与文件资产 Port 的商品服务，与品牌一同提交。
/// 成功后返回新品牌视图；临时文件校验失败或品牌与附件事务失败时不交付结果。
pub async fn product_brand_create_with_assets(
    service: CatalogService,
    req: CreateProductBrandRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductBrandView> {
    let pending = prepare_attachments(asset_requests, &actor)?;
    service.product_brand_create_with_assets(req, pending, &actor).await.map_err(Into::into)
}

/// 编辑指定品牌时，用已装配审计与文件资产 Port 的商品服务，将可选标志登记与品牌变更一同提交。
/// 标志须先上传到对象存储。临时文件校验或写入事务失败时返回错误，否则返回更新后的品牌视图。
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

/// 新增商品时，用已装配数据范围、审计与文件资产 Port 的商品服务，将已上传媒体的登记与商品一同提交。
/// 成功后返回商品视图；临时文件校验或写入事务失败时返回错误。
pub async fn product_create_with_assets(
    service: CatalogService,
    req: CreateProductRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductView> {
    let pending = prepare_attachments(asset_requests, &actor)?;
    service.product_create_with_assets(req, pending, &actor).await.map_err(Into::into)
}

/// 编辑指定商品时，用已装配数据范围、审计与文件资产 Port 的商品服务，将已上传媒体的登记与修订一同提交。
/// 成功后返回更新后的商品视图；临时文件校验或写入事务失败时返回错误。
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
