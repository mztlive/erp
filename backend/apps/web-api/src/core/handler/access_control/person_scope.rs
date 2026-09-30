//! 人员唯一业务范围接口。
use application_core::AuditActor;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use erp_identity::dto::person_scope::{PersonScopeView, SavePersonScopeRequest};
use erp_processes::adapters::scope_configuration;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "查看数据范围",
    resource = "data_scope",
    action = "list"
)]
/// 读取人员配置及有效操作。
/// # 参数
/// 人员、当前身份及应用依赖。
/// # 返回
/// 配置与策略版本。
/// # 错误
/// 越权或读取失败使用统一信封。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(user): Path<String>,
) -> Result<PersonScopeView> {
    Ok(ApiResponse::ok_with_data(
        scope_configuration(state.db(), state.rbac()).person_scopes(&user, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "创建数据范围",
    resource = "data_scope",
    action = "create"
)]
/// 替换所选操作的人员范围。
/// # 参数
/// 人员、完整选择、预期版本及当前身份。
/// # 返回
/// 原子保存成功。
/// # 错误
/// 越权、无资格、目标失效及冲突拒绝整次操作。
pub async fn save(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(user): Path<String>,
    Json(req): Json<SavePersonScopeRequest>,
) -> Result<()> {
    scope_configuration(state.db(), state.rbac()).save_person_scope(&user, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(()))
}
