use application_core::AuditActor;
use axum::extract::{Extension, Path, State};
use erp_procure_to_pay::procure_to_pay::responsibility::ProcurementResponsibilityProcess;
use erp_procurement::dto::procurement_responsibility::{
    CreateProcurementResponsibilityRuleRequest, ProcurementResponsibilityResolveRequest,
    ProcurementResponsibilityResolveView, UpdateProcurementResponsibilityRuleRequest,
};
use erp_read_models::purchase_center::procurement_responsibility::ProcurementResponsibilityReadService;
use erp_read_models::purchase_center::procurement_responsibility::dto::{
    ProcurementResponsibilityRuleListParams, ProcurementResponsibilityRulePageView,
    ProcurementResponsibilityRuleView,
};
use validator::Validate;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 分页查询采购责任规则。
pub async fn list_rules(
    State(state): State<HttpState>,
    Query(params): Query<ProcurementResponsibilityRuleListParams>,
) -> Result<ProcurementResponsibilityRulePageView> {
    params.validate()?;
    let view = ProcurementResponsibilityReadService::new(state.application().db()).rule_list(params).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 创建采购责任规则。
pub async fn create_rule(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<CreateProcurementResponsibilityRuleRequest>,
) -> Result<ProcurementResponsibilityRuleView> {
    request.validate()?;
    let view = ProcurementResponsibilityProcess::new(state.application().db(), state.application().rbac())
        .create_rule(request, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 整项更新采购责任规则。
pub async fn update_rule(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProcurementResponsibilityRuleRequest>,
) -> Result<ProcurementResponsibilityRuleView> {
    request.validate()?;
    let view = ProcurementResponsibilityProcess::new(state.application().db(), state.application().rbac())
        .update_rule(&id, request, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 逐行预览采购责任解析结果。
pub async fn resolve(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<ProcurementResponsibilityResolveRequest>,
) -> Result<ProcurementResponsibilityResolveView> {
    request.validate()?;
    let view = ProcurementResponsibilityProcess::new(state.application().db(), state.application().rbac())
        .resolve_preview(request, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
