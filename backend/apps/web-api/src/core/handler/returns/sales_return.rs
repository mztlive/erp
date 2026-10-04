//! 销售退货与拒收处理 HTTP 协议适配。

use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_processes::reverse_flow::ReturnsProcess;
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::{PageView, SalesReturnCaseListParams, SalesReturnCaseView};
use erp_returns::dto::CreateSalesReturnCaseRequest;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询销售退货处理单列表",
    resource = "sales_return_case",
    action = "list"
)]
/// 查询销售退货/拒收处理单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn sales_return_case_list(
    State(state): State<AppState>,
    Query(params): Query<SalesReturnCaseListParams>,
) -> Result<PageView<SalesReturnCaseView>> {
    let page = ReturnsReadService::new(state.db()).sales_return_case_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询销售退货处理单详情",
    resource = "sales_return_case",
    action = "detail"
)]
/// 查询销售退货/拒收处理单详情（处理单 + 明细行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 处理单 ID
///
/// # 返回
/// 返回完整处理单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn sales_return_case_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SalesReturnCaseView> {
    let view = ReturnsReadService::new(state.db()).sales_return_case_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "建立销售退货处理单",
    resource = "sales_return_case",
    action = "create"
)]
/// 建立销售退货/拒收处理单与明细行（跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建处理单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn sales_return_case_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSalesReturnCaseRequest>,
) -> Result<SalesReturnCaseView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .create_sales_return_case(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
