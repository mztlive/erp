//! 域 D06 `access_control` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::access_control` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    access_control::{
        AccessControlService, AssignUserRoleRequest, AuditEventListParams, AuditEventView,
        CreateDataScopeRequest, CreatePermissionRequest, DataScopeListParams, DataScopeView, PageView,
        PermissionListParams, PermissionView, RevokeUserRoleRequest, UpdatePermissionRequest,
        UserRoleListParams, UserRoleView,
    },
    audit::AuditActor,
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "查询权限定义列表",
    resource = "permission",
    action = "list"
)]
/// 查询权限定义列表（权限目录）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`resource`/`disabled`/`system`）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn permission_list(
    State(state): State<AppState>,
    Query(params): Query<PermissionListParams>,
) -> Result<PageView<PermissionView>> {
    let page = AccessControlService::new(state.db())
        .permission_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "创建权限定义",
    resource = "permission",
    action = "create"
)]
/// 创建权限定义（`resource:action` 唯一）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ resource, action, name }`）
///
/// # 返回
/// 返回新建的权限定义视图。
pub async fn permission_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePermissionRequest>,
) -> Result<PermissionView> {
    let view = AccessControlService::new(state.db())
        .create_permission(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "更新权限定义",
    resource = "permission",
    action = "update"
)]
/// 更新权限定义（乐观锁：请求携带期望版本，冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 权限定义 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后的权限定义视图。
pub async fn permission_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePermissionRequest>,
) -> Result<PermissionView> {
    let view = AccessControlService::new(state.db())
        .update_permission(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "删除权限定义",
    resource = "permission",
    action = "delete"
)]
/// 删除权限定义（软删除；系统内建权限禁止删除）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 权限定义 ID
///
/// # 返回
/// 无返回值。
pub async fn permission_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    AccessControlService::new(state.db())
        .delete_permission(&id, &actor)
        .await?;

    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "查询数据范围列表",
    resource = "data_scope",
    action = "list"
)]
/// 查询数据范围列表（携带 `subject_id` 时按主体查询）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`subject_type`/`scope_type`/`subject_id`）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn data_scope_list(
    State(state): State<AppState>,
    Query(params): Query<DataScopeListParams>,
) -> Result<PageView<DataScopeView>> {
    let page = AccessControlService::new(state.db())
        .data_scope_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "创建数据范围",
    resource = "data_scope",
    action = "create"
)]
/// 创建数据范围（同主体同范围类型唯一）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ subject_type, subject_id, scope_type, scope_targets }`）
///
/// # 返回
/// 返回新建的数据范围视图。
pub async fn data_scope_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDataScopeRequest>,
) -> Result<DataScopeView> {
    let view = AccessControlService::new(state.db())
        .create_data_scope(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "删除数据范围",
    resource = "data_scope",
    action = "delete"
)]
/// 删除数据范围（软删除）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 数据范围 ID
///
/// # 返回
/// 无返回值。
pub async fn data_scope_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    AccessControlService::new(state.db())
        .delete_data_scope(&id, &actor)
        .await?;

    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "查询用户角色绑定",
    resource = "user_role",
    action = "list"
)]
/// 按用户查询角色绑定（含撤权历史，只读展示）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 查询参数（`user_id` 必填）
///
/// # 返回
/// 返回绑定视图列表。
pub async fn user_role_list(
    State(state): State<AppState>,
    Query(params): Query<UserRoleListParams>,
) -> Result<Vec<UserRoleView>> {
    let items = AccessControlService::new(state.db())
        .user_role_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(items))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "分配用户角色",
    resource = "user_role",
    action = "create"
)]
/// 分配用户角色（绑定记录 + 审计事件原子写入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 分配请求（`{ user_id, role_id }`）
///
/// # 返回
/// 返回新建的绑定视图。
pub async fn user_role_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<AssignUserRoleRequest>,
) -> Result<UserRoleView> {
    let view = AccessControlService::new(state.db())
        .assign_user_role(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "撤权用户角色",
    resource = "user_role",
    action = "revoke"
)]
/// 撤权用户角色（立即紧急撤权，必须记录结构化原因）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 绑定 ID
/// * `req` - 撤权请求（含期望版本与原因）
///
/// # 返回
/// 返回撤权后的绑定视图。
pub async fn user_role_revoke(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RevokeUserRoleRequest>,
) -> Result<UserRoleView> {
    let view = AccessControlService::new(state.db())
        .revoke_user_role(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "查询审计事件列表",
    resource = "audit_event",
    action = "list"
)]
/// 查询审计事件列表（事实型留痕，不可编辑不可删除）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`actor_id`/`action_type`/`object_type`/`result`）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn audit_event_list(
    State(state): State<AppState>,
    Query(params): Query<AuditEventListParams>,
) -> Result<PageView<AuditEventView>> {
    let page = AccessControlService::new(state.db())
        .audit_event_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}
