//! 履约确认命令：在同一事务中登记已上传的凭证文件。

use application_core::AuditActor;
use erp_fulfillment::dto::{
    ConfirmElectronicDeliveryRequest, ConfirmServiceFulfillmentRequest, ElectronicDeliveryView,
    ServiceFulfillmentView,
};
use erp_fulfillment::entity::fulfillment::ServiceEvidencePolicy;
use erp_support::PendingFileAssetRequest;

use super::pending::PendingFileAssets;
use crate::Result;
use crate::fulfillment_execution::FulfillmentProcess;
use crate::fulfillment_execution::service_crypto::evidence_metadata;

/// 确认服务履约，并在同一事务中持久化已上传的凭证图片。
///
/// # 参数
/// * `service` - 组合根已装配的履约流程。
/// * `id` - 服务履约主键。
/// * `req` - 确认命令。
/// * `asset_requests` - 已写入对象存储的凭证文件。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回确认后的服务履约视图。
///
/// # 错误
/// 凭证策略、临时引用、业务规则或事务失败时返回错误。
pub async fn confirm_service_fulfillment_with_assets(
    service: FulfillmentProcess,
    id: String,
    req: ConfirmServiceFulfillmentRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ServiceFulfillmentView> {
    for request in &asset_requests {
        let (sensitivity, retention) =
            evidence_metadata(request.registration.sensitivity_class, request.registration.retention_class);
        ServiceEvidencePolicy::validate(&request.registration.content_type, sensitivity, retention, false)
            .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    }
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    service.confirm_service_fulfillment_with_assets(&id, req, pending, &actor).await
}

/// 确认电子交付，并在同一事务中持久化已上传的凭证图片。
///
/// # 参数
/// * `service` - 组合根已装配的履约流程。
/// * `id` - 电子交付主键。
/// * `req` - 电子交付确认命令。
/// * `asset_requests` - 已写入对象存储的凭证文件。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回确认后的电子交付视图。
///
/// # 错误
/// 凭证策略、临时引用、业务规则或事务失败时返回错误。
pub async fn confirm_electronic_delivery_with_assets(
    service: FulfillmentProcess,
    id: String,
    req: ConfirmElectronicDeliveryRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<ElectronicDeliveryView> {
    for request in &asset_requests {
        let (sensitivity, retention) =
            evidence_metadata(request.registration.sensitivity_class, request.registration.retention_class);
        ServiceEvidencePolicy::validate(&request.registration.content_type, sensitivity, retention, false)
            .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    }
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    service.confirm_electronic_delivery(&id, req, pending, &actor).await
}
