//! 独立对象目录协议适配。
use application_core::AuditActor;
use application_core::directory::{DirectoryPage, DirectoryQuery, DirectorySelectedQuery};
use axum::Extension;
use axum::extract::State;

use crate::http::errors::Result;
use crate::http::extractor::Query;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::warehouse_directory;

/// 查询独立目录。
/// # 参数
/// 状态、已认证身份与目录条件。
/// # 返回
/// 目录页。
/// # 错误
/// 权限、范围、参数或读取失败时传播。
pub async fn list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<DirectoryQuery>,
) -> Result<DirectoryPage> {
    let page = warehouse_directory(state.application().db(), state.application().authorization())
        .list(actor, query)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 回显当前可读的已选对象。
/// # 参数
/// 状态、已认证身份与最多100个已选ID。
/// # 返回
/// 当前可读对象页。
/// # 错误
/// 权限、范围、参数或读取失败时传播。
pub async fn selected(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<DirectorySelectedQuery>,
) -> Result<DirectoryPage> {
    let page = warehouse_directory(state.application().db(), state.application().authorization())
        .list(actor, query.into())
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}
