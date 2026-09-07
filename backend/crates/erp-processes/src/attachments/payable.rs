//! Payable commands that register bank-receipt files in the same posting transaction.

use std::sync::Arc;

use crate::finance_posting::payable::PayableService;
use crate::finance_posting::payable::SupplierPaymentWithAssetsResult;
use crate::Result;
use application_core::AuditActor;
use erp_finance::dto::payable::CommitSupplierPaymentRequest;
use erp_support::{BankReceiptEvidencePolicy, PendingFileAssetRequest};
use erp_workflow::ApprovalObjectReadPort;
use mongodb::Database;

use super::pending::PendingFileAssets;

/// Commit a supplier payment and persist uploaded bank receipts atomically.
pub async fn commit_supplier_payment_with_assets(
    db: Database,
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
        .with_object_read(object_read)
        .commit_supplier_payment_with_assets(req, pending, &actor)
        .await
}
