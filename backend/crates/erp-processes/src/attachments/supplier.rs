//! Supplier-profile commands that register qualification files in the same transaction.

use std::sync::Arc;

use application_core::AuditActor;
use erp_party::SensitiveDataCodec;
use erp_supplier::SaveSupplierProfileRequest;
use erp_support::PendingFileAssetRequest;
use mongodb::Database;

use super::pending::PendingFileAssets;
use crate::Result;
use crate::supplier_profile::{SupplierProfileService, SupplierProfileWithAssetsResult};

/// Create a supplier profile and persist uploaded qualification files atomically.
pub async fn supplier_profile_create_with_assets(
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    req: SaveSupplierProfileRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<SupplierProfileWithAssetsResult> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    SupplierProfileService::new(db, sensitive_data).create_with_assets(req, pending, &actor).await
}

/// Update a supplier profile and persist uploaded qualification files atomically.
pub async fn supplier_profile_update_with_assets(
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    id: String,
    req: SaveSupplierProfileRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<SupplierProfileWithAssetsResult> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    SupplierProfileService::new(db, sensitive_data).update_with_assets(&id, req, pending, &actor).await
}
