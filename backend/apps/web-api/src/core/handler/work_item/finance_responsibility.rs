//! 财务责任规则 HTTP 适配层。

use axum::{
    extract::{Extension, Path, State},
    Json,
};
use services::audit::AuditActor;
use services::work_item::{
    CreateFinanceResponsibilityRuleRequest, FinanceResponsibilityOwnerOptionView,
    FinanceResponsibilityRuleView, UpdateFinanceResponsibilityRuleRequest, WorkItemService,
};
use validator::Validate;

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

/// 查询财务责任规则。
#[permission_macros::permission(
    group = "财务责任管理",
    group_desc = "维护付款与销项开票的具体负责人规则",
    desc = "查询财务责任规则",
    resource = "finance_responsibility",
    action = "list"
)]
pub async fn finance_responsibility_rule_list(
    State(state): State<AppState>,
) -> Result<Vec<FinanceResponsibilityRuleView>> {
    let views = WorkItemService::new(state.db(), state.rbac())
        .finance_responsibility_rule_list()
        .await?;
    Ok(ApiResponse::ok_with_data(views))
}

/// 创建财务责任规则。
#[permission_macros::permission(
    group = "财务责任管理",
    group_desc = "维护付款与销项开票的具体负责人规则",
    desc = "创建财务责任规则",
    resource = "finance_responsibility",
    action = "manage"
)]
pub async fn finance_responsibility_rule_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<CreateFinanceResponsibilityRuleRequest>,
) -> Result<FinanceResponsibilityRuleView> {
    request.validate()?;
    let view = WorkItemService::new(state.db(), state.rbac())
        .create_finance_responsibility_rule(request, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 整项更新财务责任规则。
#[permission_macros::permission(
    group = "财务责任管理",
    group_desc = "维护付款与销项开票的具体负责人规则",
    desc = "更新财务责任规则",
    resource = "finance_responsibility",
    action = "manage"
)]
pub async fn finance_responsibility_rule_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<UpdateFinanceResponsibilityRuleRequest>,
) -> Result<FinanceResponsibilityRuleView> {
    request.validate()?;
    let view = WorkItemService::new(state.db(), state.rbac())
        .update_finance_responsibility_rule(&id, request, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询付款与销项开票的负责人候选。
#[permission_macros::permission(
    group = "财务责任管理",
    group_desc = "维护付款与销项开票的具体负责人规则",
    desc = "查询财务负责人候选",
    resource = "finance_responsibility",
    action = "list"
)]
pub async fn finance_responsibility_owner_options(
    State(state): State<AppState>,
) -> Result<Vec<FinanceResponsibilityOwnerOptionView>> {
    let views = WorkItemService::new(state.db(), state.rbac())
        .finance_responsibility_owner_options()
        .await?;
    Ok(ApiResponse::ok_with_data(views))
}
