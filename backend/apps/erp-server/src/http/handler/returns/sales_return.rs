//! 销售退货与拒收处理 HTTP 协议适配。

use application_core::{AuditActor, PageView};
use axum::Extension;
use axum::extract::{Path, State};
use erp_returns::dto::{CreateSalesReturnCaseRequest, SalesReturnCaseListParams, SalesReturnCaseView};
use erp_returns::service::sales_return_query::SalesReturnQueryService;
use erp_reverse_flow::reverse_flow::ReturnsProcess;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::local_reverse_flow_ports;

/// 按请求条件查询销售退货/拒收处理单列表。
pub async fn sales_return_case_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SalesReturnCaseListParams>,
) -> Result<PageView<SalesReturnCaseView>> {
    let page = SalesReturnQueryService::new(state.application().db(), state.application().authorization())
        .sales_return_case_list(&actor, &params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询销售退货/拒收处理单详情（处理单 + 明细行）。
/// 返回完整处理单视图。
pub async fn sales_return_case_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesReturnCaseView> {
    let view = SalesReturnQueryService::new(state.application().db(), state.application().authorization())
        .sales_return_case_detail(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 建立销售退货/拒收处理单与明细行（跨集合事务）。
/// 返回新建处理单视图。
pub async fn sales_return_case_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSalesReturnCaseRequest>,
) -> Result<SalesReturnCaseView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .create_sales_return_case(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}
