use axum::extract::{Query, State};
use services::{
    audit::{AuditLogItem, AuditLogListParams, AuditLogService},
    Page,
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "系统审计",
    group_desc = "系统操作日志",
    desc = "查询审计日志",
    resource = "audit_log",
    action = "list"
)]
/// 查询审计日志列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 查询参数
///
/// # 返回值
/// 返回分页审计日志列表
pub async fn list_audit_logs(
    State(state): State<AppState>,
    Query(params): Query<AuditLogListParams>,
) -> Result<Page<AuditLogItem>> {
    let page = AuditLogService::new(state.db()).audit_log_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}
