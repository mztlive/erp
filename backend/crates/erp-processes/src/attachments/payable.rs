//! Payable commands that register bank-receipt files in the same posting transaction.

use std::sync::Arc;

use application_core::AuditActor;
use erp_finance::dto::payable::CommitSupplierPaymentRequest;
use erp_identity::SharedRbacService;
use erp_support::{BankReceiptEvidencePolicy, PendingFileAssetRequest};
use erp_workflow::ApprovalObjectReadPort;
use mongodb::Database;

use super::pending::PendingFileAssets;
use crate::Result;
use crate::finance_posting::payable::{PayableService, SupplierPaymentWithAssetsResult};

/// Commit a supplier payment and persist uploaded bank receipts atomically.
///
/// # Parameters
/// * `rbac` - composition-root authorization reader; the policy transaction remains local
/// * `asset_requests` - validated upload metadata to persist with this payment
///
/// # Returns
/// Returns the stable payment and whether this attempt registered its uploaded assets.
///
/// # Errors
/// Returns receipt-policy, command, authorization or transaction errors unchanged.
pub async fn commit_supplier_payment_with_assets(
    db: Database,
    rbac: SharedRbacService,
    object_read: Arc<dyn ApprovalObjectReadPort>,
    req: CommitSupplierPaymentRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<SupplierPaymentWithAssetsResult> {
    for request in &asset_requests {
        BankReceiptEvidencePolicy::validate(
            &request.registration.content_type,
            request.registration.sensitivity_class,
            request.registration.retention_class,
            false,
        )
        .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    }
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    PayableService::new(db)
        .with_rbac(rbac)
        .with_object_read(object_read)
        .commit_supplier_payment_with_assets(req, pending, &actor)
        .await
}
