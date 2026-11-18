use application_core::AuditActor;
use axum::extract::{Extension, Path, State};
use erp_identity::{CreateRoleParams, RoleItem, UpdateRoleParams};
use validator::Validate;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 创建角色并写入 Casbin 权限策略，返回角色身份用于继续配置范围。
/// 新角色 ID；客户端继续打开该角色编辑页。
/// 参数、授权或持久化校验失败时返回统一错误。
pub async fn create_role(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateRoleParams>,
) -> Result<String> {
    req.validate()?;
    let role =
        state.application().rbac().create_role(req, actor, state.application().authorization()).await?;

    Ok(ApiResponse::ok_with_data(role.base.id.to_string()))
}

/// 为角色管理读取现有角色及对应 Casbin 策略，便于核对各角色已配置的操作权限。
pub async fn list_roles(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<Vec<RoleItem>> {
    let items = state.application().rbac().role_list(&actor, state.application().authorization()).await?;
    Ok(ApiResponse::ok_with_data(items))
}

/// 为账号分配角色时读取当前可选的启用角色；不返回角色管理的完整策略清单。
pub async fn list_assignable_roles(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<Vec<RoleItem>> {
    let items =
        state.application().rbac().assignable_role_list(&actor, state.application().authorization()).await?;
    Ok(ApiResponse::ok_with_data(items))
}

/// 在维护已有角色时将路径身份和更新命令交给身份服务，同步修改角色及 Casbin 策略。
pub async fn update_role(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateRoleParams>,
) -> Result<()> {
    req.validate()?;
    state.application().rbac().update_role(&id, req, actor, state.application().authorization()).await?;

    Ok(ApiResponse::<()>::ok())
}

/// 移除不再使用的非系统角色，同时处理其 Casbin 策略及绑定；系统角色不走删除流程。
pub async fn delete_role(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    state.application().rbac().delete_role(&id, actor, state.application().authorization()).await?;

    Ok(ApiResponse::<()>::ok())
}
