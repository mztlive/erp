//! 工作台动态仅沿当前任务资格读取。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::adapters::sales_timeline_service;
use erp_processes::adapters::workflow::workflow_auth;
use erp_read_models::WorkbenchReadService;
use erp_read_models::sales_timeline::{TaskTimeline, TimelineQuery};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Query;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "查询本人有权查看的待办详情",
    resource = "work_item",
    action = "detail"
)]
/// 读取当前任务的办理动态及可查看的销售单入口。
/// # 参数
/// `state`、`actor` 为应用与身份；`id`、`query` 为任务及分页。
/// # 返回
/// 返回任务动态，销售单入口另行校验详情权限。
/// # 错误
/// 任务资格失效或读取失败时返回统一错误。
pub async fn task_timeline(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<TaskTimeline> {
    let workbench = WorkbenchReadService::new(state.db(), workflow_auth(state.db(), state.rbac()));
    let view = sales_timeline_service(state.db(), state.rbac()).task(workbench, &actor, &id, query).await?;
    Ok(ApiResponse::ok_with_data(view))
}
