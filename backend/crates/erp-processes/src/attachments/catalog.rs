//! Catalog commands that register uploaded files in the same business transaction.

use application_core::AuditActor;
use erp_support::PendingFileAssetRequest;
use mongodb::Database;
use services::catalog::{
    CatalogService, CreateProductBrandRequest, CreateProductRequest, ProductBrandView, ProductView,
    UpdateProductBrandRequest, UpdateProductRequest,
};
use services::Result;

use super::pending::PendingFileAssets;

/// Create a product brand and persist any uploaded logo in one transaction.
pub async fn product_brand_create_with_assets(
    db: Database,
    req: CreateProductBrandRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductBrandView> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    CatalogService::new(db)
        .product_brand_create_with_assets(req, pending, &actor)
        .await
}

/// Update a product brand and persist any uploaded logo in one transaction.
pub async fn product_brand_update_with_assets(
    db: Database,
    id: String,
    req: UpdateProductBrandRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductBrandView> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    CatalogService::new(db)
        .product_brand_update_with_assets(&id, req, pending, &actor)
        .await
}

/// Create a product and persist uploaded media in one transaction.
pub async fn product_create_with_assets(
    db: Database,
    req: CreateProductRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductView> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    CatalogService::new(db)
        .product_create_with_assets(req, pending, &actor)
        .await
}

/// Update a product and persist uploaded media in one transaction.
pub async fn product_update_with_assets(
    db: Database,
    id: String,
    req: UpdateProductRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductView> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    CatalogService::new(db)
        .product_update_with_assets(&id, req, pending, &actor)
        .await
}
