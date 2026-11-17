use application_core::{AuditActor, Page};
use axum::Extension;
use axum::extract::State;
use erp_audit::{AuditLogItem, AuditLogListParams, AuditLogReadService};

use crate::http::errors::Result;
use crate::http::extractor::Query;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询审计日志列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `query` - 查询参数
///
/// # 返回
/// 返回分页审计日志列表。
///
/// # 错误
/// 无审计日志查询资格、参数非法或读取失败时拒绝。
pub async fn list_audit_logs(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<AuditLogListParams>,
) -> Result<Page<AuditLogItem>> {
    let page = AuditLogReadService::new(state.application().db(), state.application().authorization())
        .audit_log_list(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}
