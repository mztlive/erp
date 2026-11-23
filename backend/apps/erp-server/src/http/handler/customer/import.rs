//! 客户中心的批量建档入口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_data_import::customer_import::{CustomerImportRequest, CustomerImportResultRow};
use erp_support::{BackgroundJobView, ItemStatusFilter};
use serde::Deserialize;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 提交后台任务，并返回持久化的登记结果。
pub async fn submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<CustomerImportRequest>,
) -> Result<BackgroundJobView> {
    let job = state.application().customer_import_process().submit(request, &actor).await?;
    Ok(ApiResponse::ok_with_data(job))
}

#[derive(Deserialize)]
pub struct ResultParams {
    status: ItemStatusFilter,
}

/// 按结果分类读取原提交人的 Excel 源行。
pub async fn result_rows(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<ResultParams>,
) -> Result<Vec<CustomerImportResultRow>> {
    let rows = state.application().customer_import_process().result_rows(&id, params.status, &actor).await?;
    Ok(ApiResponse::ok_with_data(rows))
}
