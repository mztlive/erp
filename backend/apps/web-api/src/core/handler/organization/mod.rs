//! 内部组织协议适配；组织授权和变更校验由身份域执行。

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};
use application_core::AuditActor;
use axum::{extract::State, Extension, Json};
use erp_identity::entity::organization_change::{
    OrganizationChangeReceipt, OrganizationChangeRequest, OrganizationState,
};
use erp_processes::adapters::organization_service;

/// 查询组织管理范围内的组织和关系。
///
/// # 错误
/// 无权限及状态读取错误沿用统一响应。
#[permission_macros::permission(
    group = "组织管理",
    group_desc = "内部组织、成员与管理关系",
    desc = "查询组织",
    resource = "org_unit",
    action = "list"
)]
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<OrganizationState> {
    Ok(ApiResponse::ok_with_data(
        organization_service(state.db(), state.rbac())
            .state(&actor)
            .await?,
    ))
}

/// 预览组织变更影响，提交前不写入任何关系。
///
/// # 错误
/// 版本、边界或业务约束失败时返回统一错误。
#[permission_macros::permission(
    group = "组织管理",
    group_desc = "内部组织、成员与管理关系",
    desc = "管理组织",
    resource = "org_unit",
    action = "manage"
)]
pub async fn preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<OrganizationChangeRequest>,
) -> Result<OrganizationChangeReceipt> {
    Ok(ApiResponse::ok_with_data(
        organization_service(state.db(), state.rbac())
            .preview(&actor, request)
            .await?,
    ))
}

/// 提交已预览的组织变更，事务内独立重验版本和权限。
///
/// # 错误
/// 冲突及越权不产生部分变更。
#[permission_macros::permission(
    group = "组织管理",
    group_desc = "内部组织、成员与管理关系",
    desc = "管理组织",
    resource = "org_unit",
    action = "manage"
)]
pub async fn change(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<OrganizationChangeRequest>,
) -> Result<OrganizationChangeReceipt> {
    Ok(ApiResponse::ok_with_data(
        organization_service(state.db(), state.rbac())
            .change(&actor, request)
            .await?,
    ))
}
