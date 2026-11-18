//! 客户退款 HTTP 协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::{CustomerRefundListParams, CustomerRefundView, PageView};
use erp_returns::dto::{
    CancelCustomerRefundApprovalRequest, CommitCustomerRefundRequest, CreateCustomerRefundRequest,
    PostCustomerRefundRequest, SubmitCustomerRefundRequest,
};
use erp_returns::service::ReturnsService;
use erp_reverse_flow::reverse_flow::ReturnsProcess;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::local_reverse_flow_ports;

/// 按请求条件查询客户退款列表。
pub async fn customer_refund_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CustomerRefundListParams>,
) -> Result<PageView<CustomerRefundView>> {
    let page = ReturnsReadService::new(state.application().db(), state.application().authorization())
        .customer_refund_list(&actor, &params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询客户退款详情，将认证操作人与路径对象交给读模型校验。
/// 返回退款单视图。
pub async fn customer_refund_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CustomerRefundView> {
    let view = ReturnsReadService::new(state.application().db(), state.application().authorization())
        .customer_refund_detail(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记客户退款草稿（退款单号唯一构成幂等去重；经办/复核分离）。
/// 返回新建退款单视图。
pub async fn customer_refund_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerRefundRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .create_customer_refund(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按原回款一次创建客户退款并启动审批。
/// `req` 为业务命令与期望版本。
pub async fn customer_refund_commit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitCustomerRefundRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .commit_customer_refund(req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 提交客户退款并启动统一审批。客户端不得选择定义或审批人。
/// `req` 为提交请求（版本与幂等键）。
/// 返回提交后的退款单视图。
pub async fn customer_refund_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitCustomerRefundRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .submit_customer_refund(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 撤回尚未最终通过的客户退款审批。
/// `req` 为撤回请求（原因必填）。
/// 返回撤回后的退款单视图。
pub async fn customer_refund_cancel_approval(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelCustomerRefundApprovalRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .cancel_customer_refund_approval(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 兼容旧的客户退款直接过账入口，始终返回冲突，不接受客户端据此改写业务事实。
/// 实际过账只在审批最终通过动作中执行。
pub async fn customer_refund_post(
    State(_state): State<HttpState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostCustomerRefundRequest>,
) -> Result<CustomerRefundView> {
    match ReturnsService::reject_client_post() {
        Err(error) => Err(error.into()),
        Ok(never) => match never {},
    }
}
