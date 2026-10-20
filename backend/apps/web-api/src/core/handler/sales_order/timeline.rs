//! 销售单全流程展示入口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, Query, State};
use erp_processes::adapters::sales_timeline_service;
use erp_read_models::sales_timeline::{TimelinePage, TimelineQuery};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "查询销售单详情",
    resource = "sales_order",
    action = "detail"
)]
/// 读取销售单及关联单据的已授权时间线。
/// # 参数
/// `state`、`actor` 为应用与身份；`id`、`query` 为销售单及分页。
/// # 返回
/// 返回业务动态与下一页游标。
/// # 错误
/// 不可见、参数无效或底层故障时返回统一错误。
pub async fn sales_timeline(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<TimelinePage> {
    let view = sales_timeline_service(state.db(), state.rbac()).sales(&actor, &id, query).await?;
    Ok(ApiResponse::ok_with_data(view))
}
