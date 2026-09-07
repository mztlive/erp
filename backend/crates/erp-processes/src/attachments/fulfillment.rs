//! Fulfillment commands that register on-site evidence files in the same transaction.

use crate::fulfillment_execution::service_crypto::evidence_metadata;
use crate::fulfillment_execution::FulfillmentProcess;
use crate::Result;
use application_core::AuditActor;
use erp_fulfillment::dto::{ConfirmServiceFulfillmentRequest, ServiceFulfillmentView};
use erp_fulfillment::entity::fulfillment::ServiceEvidencePolicy;
use erp_support::PendingFileAssetRequest;

use super::pending::PendingFileAssets;

/// Confirm a service fulfillment and persist uploaded evidence images atomically.
///
/// # Parameters
/// * `service` - fulfillment service already wired by the composition root
/// * `id` - service fulfillment id
/// * `req` - confirmation command
/// * `asset_requests` - files already written to object storage
/// * `actor` - authenticated audit actor
///
/// # Errors
/// Evidence policy, reference, business-rule or transaction failures.
pub async fn confirm_service_fulfillment_with_assets(
    service: FulfillmentProcess,
    id: String,
    req: ConfirmServiceFulfillmentRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ServiceFulfillmentView> {
    for request in &asset_requests {
        let (sensitivity, retention) = evidence_metadata(
            request.registration.sensitivity_class,
            request.registration.retention_class,
        );
        ServiceEvidencePolicy::validate(&request.registration.content_type, sensitivity, retention, false)
            .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    }
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    service
        .confirm_service_fulfillment_with_assets(&id, req, pending, &actor)
        .await
}
