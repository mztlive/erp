//! 本人发起审批的只读动态。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::adapters::sales_timeline_service;
use erp_read_models::sales_timeline::{TaskTimeline, TimelineQuery};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Query;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "查询审批实例详情",
    resource = "approval_instance",
    action = "read"
)]
/// 沿实例阅读授权返回相关业务动态。
/// # 参数
/// `state`、`actor` 为应用与身份；`id`、`query` 为实例及分页。
/// # 返回
/// 返回经前后授权重验的动态。
/// # 错误
/// 实例不可见、来源变化或读取失败时返回统一错误。
pub async fn approval_timeline(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<TaskTimeline> {
    let runtime = state.approval_runtime_service();
    let view = sales_timeline_service(state.db(), state.rbac())
        .approval(runtime.as_ref(), &actor, &id, query)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
