//! 后台登录即可读取的结算主体目录；只返回最低识别字段。
use application_core::AuditActor;
use application_core::directory::{DirectoryQuery, DirectorySelectedQuery};
use axum::Extension;
use axum::extract::State;
use erp_party::dto::directory::SettlementPartyDirectoryPage;
use erp_party::service::directory::SettlementPartyDirectoryService;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Query;
use crate::core::response::ApiResponse;

/// 查询独立目录。
/// # 参数
/// 状态、已认证身份与目录条件。
/// # 返回
/// 目录页。
/// # 错误
/// 参数、目录版本或读取失败时传播；后台身份由认证中间件校验。
pub async fn list(
    State(state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Query(query): Query<DirectoryQuery>,
) -> Result<SettlementPartyDirectoryPage> {
    let page = SettlementPartyDirectoryService::new(state.db()).list(query).await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 回显当前可读的已选对象。
/// # 参数
/// 状态、已认证身份与最多100个已选ID。
/// # 返回
/// 当前可读对象页。
/// # 错误
/// 参数、目录版本或读取失败时传播；后台身份由认证中间件校验。
pub async fn selected(
    State(state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Query(query): Query<DirectorySelectedQuery>,
) -> Result<SettlementPartyDirectoryPage> {
    let page = SettlementPartyDirectoryService::new(state.db()).list(query.into()).await?;
    Ok(ApiResponse::ok_with_data(page))
}
