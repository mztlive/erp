//! 独立对象目录协议适配。
use application_core::AuditActor;
use application_core::directory::{DirectoryPage, DirectoryQuery, DirectorySelectedQuery};
use axum::Extension;
use axum::extract::{Query, State};
use erp_processes::adapters::warehouse_directory;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

/// 查询独立目录。
/// # 参数
/// 状态、已认证身份与目录条件。
/// # 返回
/// 目录页。
/// # 错误
/// 权限、范围、参数或读取失败时传播。
#[permission_macros::permission(
    group = "对象目录",
    group_desc = "按独立范围查询业务对象",
    desc = "查询仓库目录",
    resource = "warehouse",
    action = "list"
)]
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<DirectoryQuery>,
) -> Result<DirectoryPage> {
    let page = warehouse_directory(state.db(), state.rbac()).list(actor, query).await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 回显当前可读的已选对象。
/// # 参数
/// 状态、已认证身份与最多100个已选ID。
/// # 返回
/// 当前可读对象页。
/// # 错误
/// 权限、范围、参数或读取失败时传播。
#[permission_macros::permission(
    group = "对象目录",
    group_desc = "按独立范围查询业务对象",
    desc = "回显warehouse目录",
    resource = "warehouse",
    action = "list"
)]
pub async fn selected(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<DirectorySelectedQuery>,
) -> Result<DirectoryPage> {
    let page = warehouse_directory(state.db(), state.rbac()).list(actor, query.into()).await?;
    Ok(ApiResponse::ok_with_data(page))
}
