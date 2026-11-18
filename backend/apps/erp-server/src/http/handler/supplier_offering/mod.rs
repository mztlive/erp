//! 供应商供给 HTTP Handler。
//!
//! Handler 只做协议适配、成本字段授权与 Service 调用，不定义重复 DTO。

pub mod batch;
pub mod delivery_template;
pub mod read;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_master_data::offering_handover::OfferingHandoverPorts;
use erp_master_data::{handover_offering, offering_handover_candidates as load_offering_handover_candidates};
use erp_permissions::internal;
use erp_read_models::supplier_center::offering::dto::SupplierOfferingListParams;
use erp_read_models::supplier_center::{
    MongoOfferingProcurementOwners, SupplierOfferingListView, SupplierOfferingReadService,
};
use erp_supply::dto::supplier_offering::{
    CompleteSupplierSupplyExceptionTaskRequest, CompleteSupplierSupplyExceptionTaskResult,
    CreateSupplierOfferingRequest, CreateSupplierOfferingResult, ReviseSupplierOfferingRequest,
    ReviseSupplierOfferingResult, UpdateSupplierOfferingAvailabilityRequest,
    UpdateSupplierOfferingAvailabilityResult,
};
use erp_supply::{
    HandoverCandidateView, HandoverSupplierOfferingRequest, HandoverSupplierOfferingView, OfferingAccess,
};
use persistence_core::NoTransaction;

use crate::http::errors::{Error, Result};
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::{offering_access, scoped_offering_service};

/// 按请求条件查询供应商供给列表。
/// 返回供给分页列表；无成本权限时自动清除成本字段。
pub async fn list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SupplierOfferingListParams>,
) -> Result<SupplierOfferingListView> {
    let mut page = SupplierOfferingReadService::new(
        state.application().db(),
        OfferingAccess::new(state.application().db(), state.application().authorization()),
        MongoOfferingProcurementOwners::shared(state.application().db()),
    )
    .list(&params, &actor)
    .await?;
    if !can_view_costs(&state, &actor).await? {
        for item in &mut page.data.page.items {
            item.redact_costs();
        }
    }
    Ok(ApiResponse::ok_with_data(page))
}

/// 为公司 SKU 新增供应商供给。
/// 返回新供给及首版事实主键。
pub async fn create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierOfferingRequest>,
) -> Result<CreateSupplierOfferingResult> {
    let result =
        crate::wiring::scoped_offering_process(state.application().db(), state.application().authorization())
            .create(req, &actor)
            .await?;
    Ok(ApiResponse::ok_with_data(result))
}

/// 追加供给商业条款修订。
/// `req` 为新条款与期望版本。
/// 返回新修订号与供给状态。
pub async fn revise(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReviseSupplierOfferingRequest>,
) -> Result<ReviseSupplierOfferingResult> {
    let result =
        crate::wiring::scoped_offering_process(state.application().db(), state.application().authorization())
            .revise(&id, req, &actor)
            .await?;
    Ok(ApiResponse::ok_with_data(result))
}

/// 更新供给的实时可供状态与数量。
/// 返回更新后的可供状态与投影版本。
pub async fn update_availability(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSupplierOfferingAvailabilityRequest>,
) -> Result<UpdateSupplierOfferingAvailabilityResult> {
    let result =
        crate::wiring::scoped_offering_process(state.application().db(), state.application().authorization())
            .update_availability(&id, req, &actor)
            .await?;
    Ok(ApiResponse::ok_with_data(result))
}

/// 当供应停止原因与安全暂停影响已核对时，完成正式异常任务。结案不恢复供给或商品发布，安全暂停继续生效。
pub async fn complete_supply_exception_task(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CompleteSupplierSupplyExceptionTaskRequest>,
) -> Result<CompleteSupplierSupplyExceptionTaskResult> {
    let result =
        crate::wiring::scoped_offering_process(state.application().db(), state.application().authorization())
            .complete_supply_exception_task(&id, req, &actor)
            .await?;
    Ok(ApiResponse::ok_with_data(result))
}

/// 显式交接供给维护人与可选业务组织。
/// `req` 为交接请求（目标、原因、版本与幂等键）。
/// 返回交接后的维护人、组织与版本。
pub async fn offering_handover(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<HandoverSupplierOfferingRequest>,
) -> Result<HandoverSupplierOfferingView> {
    let view = handover_offering(
        state.application().db(),
        state.application().authorization(),
        OfferingHandoverPorts::new(
            offering_access(state.application().db(), state.application().authorization()),
            scoped_offering_service(state.application().db(), state.application().authorization()),
        ),
        &id,
        req,
        &actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询合格有效的供给维护人交接候选。
/// 返回有效且具备 `supplier_offering:update` 的账号。
pub async fn offering_handover_candidates(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<HandoverCandidateView>> {
    let view = load_offering_handover_candidates(
        state.application().db(),
        state.application().authorization(),
        OfferingHandoverPorts::new(
            offering_access(state.application().db(), state.application().authorization()),
            scoped_offering_service(state.application().db(), state.application().authorization()),
        ),
        &id,
        &actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

async fn can_view_costs(state: &HttpState, actor: &AuditActor) -> std::result::Result<bool, Error> {
    state
        .application()
        .authorization()
        .has_permission(actor, &internal::SUPPLIER_OFFERING_COST_DETAIL, &mut NoTransaction)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod list_query_tests {
    use axum::http::{StatusCode, Uri};

    use super::*;

    #[test]
    fn list_query_preserves_people_filters_and_rejects_organization_filters() {
        let uri: Uri = "/?owner_user_ids=user-1&procurement_owner_user_ids=buyer-1&page=1".parse().unwrap();
        let Query(params) = Query::<SupplierOfferingListParams>::try_from_uri(&uri).unwrap();
        assert_eq!(params.owner_user_ids.unwrap().as_slice(), &["user-1".to_string()]);
        assert_eq!(params.procurement_owner_user_ids.unwrap().as_slice(), &["buyer-1".to_string()]);
        for query in ["org_unit_ids=org-a", "include_descendants=true", "include_descendants=false"] {
            let uri: Uri = format!("/?{query}").parse().unwrap();
            let error = Query::<SupplierOfferingListParams>::try_from_uri(&uri).unwrap_err();
            assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        }
    }
}
