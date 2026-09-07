//! Catalog commands that register uploaded files in the same business transaction.

use crate::Result;
use application_core::AuditActor;
use erp_catalog::{
    CreateProductBrandRequest, CreateProductRequest, ProductBrandView, ProductView,
    UpdateProductBrandRequest, UpdateProductRequest,
};
use erp_support::PendingFileAssetRequest;
use mongodb::Database;

use crate::adapters::{catalog_service, CatalogPendingAttachments};

use super::pending::PendingFileAssets;

/// Create a product brand and persist any uploaded logo in one transaction.
pub async fn product_brand_create_with_assets(
    db: Database,
    req: CreateProductBrandRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductBrandView> {
    let pending =
        CatalogPendingAttachments::from_support(PendingFileAssets::prepare(asset_requests, &actor)?.shared());
    catalog_service(db)
        .product_brand_create_with_assets(req, pending, &actor)
        .await
        .map_err(Into::into)
}

/// Update a product brand and persist any uploaded logo in one transaction.
pub async fn product_brand_update_with_assets(
    db: Database,
    id: String,
    req: UpdateProductBrandRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductBrandView> {
    let pending =
        CatalogPendingAttachments::from_support(PendingFileAssets::prepare(asset_requests, &actor)?.shared());
    catalog_service(db)
        .product_brand_update_with_assets(&id, req, pending, &actor)
        .await
        .map_err(Into::into)
}

/// Create a product and persist uploaded media in one transaction.
pub async fn product_create_with_assets(
    db: Database,
    req: CreateProductRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductView> {
    let pending =
        CatalogPendingAttachments::from_support(PendingFileAssets::prepare(asset_requests, &actor)?.shared());
    catalog_service(db)
        .product_create_with_assets(req, pending, &actor)
        .await
        .map_err(Into::into)
}

/// Update a product and persist uploaded media in one transaction.
pub async fn product_update_with_assets(
    db: Database,
    id: String,
    req: UpdateProductRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ProductView> {
    let pending =
        CatalogPendingAttachments::from_support(PendingFileAssets::prepare(asset_requests, &actor)?.shared());
    catalog_service(db)
        .product_update_with_assets(&id, req, pending, &actor)
        .await
        .map_err(Into::into)
}
