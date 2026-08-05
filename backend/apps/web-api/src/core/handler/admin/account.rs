use axum::{
    extract::{Extension, Path, State},
    Json,
};
use services::{
    audit::AuditActor,
    iam::{AdminItem, AdminService, CreateAdminParams, UpdateAdminParams, UpdateAdminRoleParams},
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "账号管理",
    group_desc = "管理员账号的增删改查",
    desc = "创建管理员",
    resource = "admin",
    action = "create"
)]
pub async fn create_admin(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateAdminParams>,
) -> Result<()> {
    AdminService::new(state.db(), state.rbac())
        .create_admin(req, actor)
        .await?;

    Ok(ApiResponse::<()>::ok())
}

#[permission_macros::permission(
    group = "账号管理",
    group_desc = "管理员账号的增删改查",
    desc = "查询管理员列表",
    resource = "admin",
    action = "list"
)]
pub async fn list_admins(State(state): State<AppState>) -> Result<Vec<AdminItem>> {
    let items = AdminService::new(state.db(), state.rbac()).admin_list().await?;
    Ok(ApiResponse::ok_with_data(items))
}

#[permission_macros::permission(
    group = "账号管理",
    group_desc = "管理员账号的增删改查",
    desc = "更新管理员信息",
    resource = "admin",
    action = "update"
)]
pub async fn update_admin(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(mut req): Json<UpdateAdminParams>,
) -> Result<()> {
    req.id = id;
    AdminService::new(state.db(), state.rbac())
        .update_admin(req, actor)
        .await?;

    Ok(ApiResponse::<()>::ok())
}

#[permission_macros::permission(
    group = "账号管理",
    group_desc = "管理员账号的增删改查",
    desc = "删除管理员",
    resource = "admin",
    action = "delete"
)]
pub async fn delete_admin(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    AdminService::new(state.db(), state.rbac())
        .delete_admin(id, actor)
        .await?;

    Ok(ApiResponse::<()>::ok())
}

#[permission_macros::permission(
    group = "账号管理",
    group_desc = "管理员账号的增删改查",
    desc = "更新管理员角色",
    resource = "admin",
    action = "update_role"
)]
pub async fn update_admin_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(mut req): Json<UpdateAdminRoleParams>,
) -> Result<()> {
    req.id = id;
    AdminService::new(state.db(), state.rbac())
        .update_admin_role(req, actor)
        .await?;

    Ok(ApiResponse::<()>::ok())
}
