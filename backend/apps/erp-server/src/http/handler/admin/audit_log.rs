use application_core::Page;
use axum::extract::State;
use erp_audit::{AuditLogItem, AuditLogListParams, AuditLogService};

use crate::http::errors::Result;
use crate::http::extractor::Query;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询审计日志列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 查询参数
///
/// # 返回值
/// 返回分页审计日志列表
pub async fn list_audit_logs(
    State(state): State<HttpState>,
    Query(params): Query<AuditLogListParams>,
) -> Result<Page<AuditLogItem>> {
    let page = AuditLogService::new(state.application().db()).audit_log_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}
