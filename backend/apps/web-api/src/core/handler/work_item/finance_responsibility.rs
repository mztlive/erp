//! 财务责任规则 HTTP 适配层。

use application_core::AuditActor;
use axum::Json;
use axum::extract::{Extension, Path, State};
use erp_processes::adapters::workflow::work_item_service;
use erp_workflow::service::work_item::{
    CreateFinanceResponsibilityRuleRequest, FinanceResponsibilityOwnerOptionView,
    FinanceResponsibilityRuleView, UpdateFinanceResponsibilityRuleRequest,
};
use validator::Validate;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

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
    let views = work_item_service(state.db(), state.rbac()).finance_responsibility_rule_list().await?;
    Ok(ApiResponse::ok_with_data(views))
}

/// 创建财务责任规则。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前操作人
/// * `request` - 规则匹配范围与负责人
///
/// # 返回
/// 返回新规则视图。
///
/// # 错误
/// 请求非法、资格校验或规则创建失败时返回错误。
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
    let view = work_item_service(state.db(), state.rbac())
        .create_finance_responsibility_rule(request, actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 整项更新财务责任规则。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前操作人
/// * `id` - 规则 ID
/// * `request` - 预期版本与完整规则配置
///
/// # 返回
/// 返回更新后的规则视图。
///
/// # 错误
/// 请求非法、版本冲突、资格校验或规则更新失败时返回错误。
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
    let view = work_item_service(state.db(), state.rbac())
        .update_finance_responsibility_rule(id, request, actor)
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
    let views = work_item_service(state.db(), state.rbac()).finance_responsibility_owner_options().await?;
    Ok(ApiResponse::ok_with_data(views))
}
