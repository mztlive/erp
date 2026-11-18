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

/// 为采购责任配置分页读取已有规则，先校验查询参数，再由读模型按当前操作人权限取数。
pub async fn list_rules(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ProcurementResponsibilityRuleListParams>,
) -> Result<ProcurementResponsibilityRulePageView> {
    params.validate()?;
    let view = ProcurementResponsibilityReadService::new(
        state.application().db(),
        state.application().authorization(),
    )
    .rule_list(params, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 新增采购责任规则时校验请求结构，并将认证操作人交给采购编排执行创建。
pub async fn create_rule(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<CreateProcurementResponsibilityRuleRequest>,
) -> Result<ProcurementResponsibilityRuleView> {
    request.validate()?;
    let view =
        ProcurementResponsibilityProcess::new(state.application().db(), state.application().authorization())
            .create_rule(request, &actor)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 修改现有采购责任规则时，以路径 ID 和完整规则请求调用采购编排，不把局部输入当补丁应用。
pub async fn update_rule(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProcurementResponsibilityRuleRequest>,
) -> Result<ProcurementResponsibilityRuleView> {
    request.validate()?;
    let view =
        ProcurementResponsibilityProcess::new(state.application().db(), state.application().authorization())
            .update_rule(&id, request, &actor)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 建规则或调整规则前预览逐行责任解析结果；校验待解析请求后调用采购预览流程。
pub async fn resolve(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<ProcurementResponsibilityResolveRequest>,
) -> Result<ProcurementResponsibilityResolveView> {
    request.validate()?;
    let view =
        ProcurementResponsibilityProcess::new(state.application().db(), state.application().authorization())
            .resolve_preview(request, &actor)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}
