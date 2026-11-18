//! 履约确认命令：在同一事务中登记已上传的凭证文件。

use std::sync::Arc;

use application_core::AuditActor;
use erp_file_assets::PendingFileAssets;
use erp_fulfillment::dto::{
    ConfirmElectronicDeliveryRequest, ConfirmServiceFulfillmentRequest, ElectronicDeliveryView,
    ServiceFulfillmentView,
};
use erp_fulfillment::entity::fulfillment::ServiceEvidencePolicy;
use erp_support::{PendingAttachmentBatch, PendingFileAssetRequest};

use crate::Result;
use crate::fulfillment_execution::FulfillmentProcess;
use crate::fulfillment_execution::service_crypto::evidence_metadata;

/// 现场服务图片已上传后，按凭证政策准备本批资产，再交给履约命令同事务登记凭证及确认服务。
/// 成功返回已确认的服务履约视图；凭证策略、临时引用、业务规则或事务失败均返回错误。
pub async fn confirm_service_fulfillment_with_assets(
    service: FulfillmentProcess,
    id: String,
    req: ConfirmServiceFulfillmentRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ServiceFulfillmentView> {
    let pending = prepare_evidence_assets(asset_requests, &actor)?;
    service.confirm_service_fulfillment_with_assets(&id, req, pending, &actor).await
}

/// 电子交付图片已上传后，按凭证政策准备本批资产，再同事务登记凭证及确认电子交付。
/// 成功返回已确认的电子交付视图；凭证策略、临时引用、业务规则或事务失败均返回错误。
pub async fn confirm_electronic_delivery_with_assets(
    service: FulfillmentProcess,
    id: String,
    req: ConfirmElectronicDeliveryRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ElectronicDeliveryView> {
    let pending = prepare_evidence_assets(asset_requests, &actor)?;
    service.confirm_electronic_delivery(&id, req, pending, &actor).await
}

fn prepare_evidence_assets(
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: &AuditActor,
) -> Result<Arc<dyn PendingAttachmentBatch>> {
    for request in &asset_requests {
        let (sensitivity, retention) =
            evidence_metadata(request.registration.sensitivity_class, request.registration.retention_class);
        ServiceEvidencePolicy::validate(&request.registration.content_type, sensitivity, retention, false)
            .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    }

    Ok(PendingFileAssets::prepare(asset_requests, actor)?.shared())
}
