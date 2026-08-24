use axum::{
    extract::{Extension, Path, Query, State},
    Json,
};
use services::audit::AuditActor;
use services::procurement_responsibility::{
    CreateProcurementResponsibilityRuleRequest, ProcurementResponsibilityResolveRequest,
    ProcurementResponsibilityResolveView, ProcurementResponsibilityRuleListParams,
    ProcurementResponsibilityRulePageView, ProcurementResponsibilityRuleView,
    ProcurementResponsibilityService, UpdateProcurementResponsibilityRuleRequest,
};
use validator::Validate;

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

/// 分页查询采购责任规则。
#[permission_macros::permission(
    group = "采购责任管理",
    group_desc = "维护采购责任规则并预览逐行负责人",
    desc = "查询采购责任规则",
    resource = "procurement_responsibility",
    action = "list"
)]
pub async fn list_rules(
    State(state): State<AppState>,
    Query(params): Query<ProcurementResponsibilityRuleListParams>,
) -> Result<ProcurementResponsibilityRulePageView> {
    params.validate()?;
    let view = ProcurementResponsibilityService::new(state.db(), state.rbac())
        .rule_list(params)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 创建采购责任规则。
#[permission_macros::permission(
    group = "采购责任管理",
    group_desc = "维护采购责任规则并预览逐行负责人",
    desc = "创建采购责任规则",
    resource = "procurement_responsibility",
    action = "manage"
)]
pub async fn create_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<CreateProcurementResponsibilityRuleRequest>,
) -> Result<ProcurementResponsibilityRuleView> {
    request.validate()?;
    let view = ProcurementResponsibilityService::new(state.db(), state.rbac())
        .create_rule(request, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 整项更新采购责任规则。
#[permission_macros::permission(
    group = "采购责任管理",
    group_desc = "维护采购责任规则并预览逐行负责人",
    desc = "更新采购责任规则",
    resource = "procurement_responsibility",
    action = "manage"
)]
pub async fn update_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProcurementResponsibilityRuleRequest>,
) -> Result<ProcurementResponsibilityRuleView> {
    request.validate()?;
    let view = ProcurementResponsibilityService::new(state.db(), state.rbac())
        .update_rule(&id, request, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 逐行预览采购责任解析结果。
#[permission_macros::permission(
    group = "采购责任管理",
    group_desc = "维护采购责任规则并预览逐行负责人",
    desc = "预览采购责任解析",
    resource = "procurement_responsibility",
    action = "list"
)]
pub async fn resolve(
    State(state): State<AppState>,
    Json(request): Json<ProcurementResponsibilityResolveRequest>,
) -> Result<ProcurementResponsibilityResolveView> {
    request.validate()?;
    let view = ProcurementResponsibilityService::new(state.db(), state.rbac())
        .resolve_preview(request)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
