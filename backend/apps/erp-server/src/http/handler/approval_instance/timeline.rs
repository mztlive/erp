//! 本人发起审批的只读动态。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::sales_timeline::{TaskTimeline, TimelineQuery};

use crate::http::errors::Result;
use crate::http::extractor::Query;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::sales_timeline_service;

/// 沿实例阅读授权返回相关业务动态。
/// `state`、`actor` 为应用与身份；`id`、`query` 为实例及分页。
/// 返回经前后授权重验的动态。
/// 实例不可见、来源变化或读取失败时返回统一错误。
pub async fn approval_timeline(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<TaskTimeline> {
    let runtime = state.application().approval_runtime_service();
    let view = sales_timeline_service(
        state.application().db(),
        state.application().rbac(),
        state.application().authorization(),
    )
    .approval(runtime.as_ref(), &actor, &id, query)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}
