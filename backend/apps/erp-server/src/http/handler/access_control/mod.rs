//! 域 D06 `access_control` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_identity` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

pub mod inspection;
pub mod person_directory;
pub mod policy_bundle;
pub mod resource_grants;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_identity::{
    AssignUserRoleRequest, AuditEventListParams, AuditEventView, CreatePermissionRequest, PageView,
    PermissionListParams, PermissionView, RevokeUserRoleRequest, UpdatePermissionRequest, UserRoleListParams,
    UserRoleView,
};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::scope_configuration;

/// 查询权限定义列表（权限目录）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`resource`/`disabled`/`system`）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn permission_list(
    State(state): State<HttpState>,
    Query(params): Query<PermissionListParams>,
) -> Result<PageView<PermissionView>> {
    let page = scope_configuration(state.application().db(), state.application().rbac())
        .permission_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePermissionRequest>,
) -> Result<PermissionView> {
    let view = scope_configuration(state.application().db(), state.application().rbac())
        .create_permission(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePermissionRequest>,
) -> Result<PermissionView> {
    let view = scope_configuration(state.application().db(), state.application().rbac())
        .update_permission(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    scope_configuration(state.application().db(), state.application().rbac())
        .delete_permission(&id, &actor)
        .await?;

    Ok(ApiResponse::ok())
}

/// 按用户查询角色绑定（含撤权历史，只读展示）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 查询参数（`user_id` 必填）
///
/// # 返回
/// 返回绑定视图列表。
pub async fn user_role_list(
    State(state): State<HttpState>,
    Query(params): Query<UserRoleListParams>,
) -> Result<Vec<UserRoleView>> {
    let items = scope_configuration(state.application().db(), state.application().rbac())
        .user_role_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(items))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<AssignUserRoleRequest>,
) -> Result<UserRoleView> {
    let view = scope_configuration(state.application().db(), state.application().rbac())
        .assign_user_role(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RevokeUserRoleRequest>,
) -> Result<UserRoleView> {
    let view = scope_configuration(state.application().db(), state.application().rbac())
        .revoke_user_role(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 查询审计事件列表（事实型留痕，不可编辑不可删除）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`actor_id`/`action_type`/`object_type`/`result`）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn audit_event_list(
    State(state): State<HttpState>,
    Query(params): Query<AuditEventListParams>,
) -> Result<PageView<AuditEventView>> {
    let page = scope_configuration(state.application().db(), state.application().rbac())
        .audit_event_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

pub mod person_scope;

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use erp_identity::access_control::{UserRole, UserRoleId};
    use erp_identity::{AssignUserRoleRequest, UserRoleView};
    use serde_json::json;

    use crate::http::errors::Error;

    /// 请求中的无效时间返回 400；正常请求与响应保持秒级数字协议。
    #[test]
    fn role_assignment_time_contract() {
        for seconds in [0, u64::MAX, u64::try_from(i64::MAX).unwrap()] {
            let request: AssignUserRoleRequest = serde_json::from_value(json!({
                "user_id": "user", "role_id": "role", "effective_from": seconds
            }))
            .unwrap();
            let error = request.into_data("admin").unwrap_err();
            assert_eq!(Error::from(error).into_response().status(), StatusCode::BAD_REQUEST);
        }
        let request: AssignUserRoleRequest = serde_json::from_value(json!({
            "user_id": "user", "role_id": "role", "effective_from": 10, "effective_to": 20
        }))
        .unwrap();
        let entity = UserRole::new(UserRoleId::new("binding"), request.into_data("admin").unwrap()).unwrap();
        let response = serde_json::to_value(UserRoleView::from(entity)).unwrap();
        assert_eq!(response["effective_from"], 10);
        assert_eq!(response["effective_to"], 20);
    }
}
