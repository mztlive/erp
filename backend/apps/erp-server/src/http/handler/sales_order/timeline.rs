//! 销售单全流程展示入口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::sales_timeline::{TimelinePage, TimelineQuery};

use crate::http::errors::Result;
use crate::http::extractor::Query;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::sales_timeline_service;

/// 读取销售单及关联单据的已授权时间线。
/// # 参数
/// `state`、`actor` 为应用与身份；`id`、`query` 为销售单及分页。
/// # 返回
/// 返回业务动态与下一页游标。
/// # 错误
/// 不可见、参数无效或底层故障时返回统一错误。
pub async fn sales_timeline(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<TimelinePage> {
    let view = sales_timeline_service(state.application().db(), state.application().rbac())
        .sales(&actor, &id, query)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
