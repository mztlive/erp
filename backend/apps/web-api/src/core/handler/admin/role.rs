use axum::{
    extract::{Extension, Path, State},
    Json,
};
use services::{
    audit::AuditActor,
    iam::{CreateRoleParams, RoleItem, UpdateRoleParams},
};
use validator::Validate;

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

/// 创建角色并写入 Casbin 权限策略。
#[permission_macros::permission(
    group = "角色管理",
    group_desc = "系统角色和权限配置",
    desc = "创建角色",
    resource = "role",
    action = "create"
)]
pub async fn create_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateRoleParams>,
) -> Result<()> {
    req.validate()?;
    state.rbac().create_role(req, actor).await?;

    Ok(ApiResponse::<()>::ok())
}

/// 查询全部角色及其 Casbin 权限策略。
#[permission_macros::permission(
    group = "角色管理",
    group_desc = "系统角色和权限配置",
    desc = "查询角色列表",
    resource = "role",
    action = "list"
)]
pub async fn list_roles(State(state): State<AppState>) -> Result<Vec<RoleItem>> {
    let items = state.rbac().role_list().await?;
    Ok(ApiResponse::ok_with_data(items))
}

/// 查询当前可分配的启用角色。
#[permission_macros::permission(
    group = "角色管理",
    group_desc = "系统角色和权限配置",
    desc = "查询可分配角色",
    resource = "role",
    action = "assignable_list"
)]
pub async fn list_assignable_roles(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<Vec<RoleItem>> {
    let items = state.rbac().assignable_role_list(&actor).await?;
    Ok(ApiResponse::ok_with_data(items))
}

/// 更新角色与 Casbin 权限策略。
#[permission_macros::permission(
    group = "角色管理",
    group_desc = "系统角色和权限配置",
    desc = "更新角色信息",
    resource = "role",
    action = "update"
)]
pub async fn update_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateRoleParams>,
) -> Result<()> {
    req.validate()?;
    state.rbac().update_role(&id, req, actor).await?;

    Ok(ApiResponse::<()>::ok())
}

/// 删除非系统角色及其 Casbin policy/绑定。
#[permission_macros::permission(
    group = "角色管理",
    group_desc = "系统角色和权限配置",
    desc = "删除角色",
    resource = "role",
    action = "delete"
)]
pub async fn delete_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    state.rbac().delete_role(&id, actor).await?;

    Ok(ApiResponse::<()>::ok())
}
