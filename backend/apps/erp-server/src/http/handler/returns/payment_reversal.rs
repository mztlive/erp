//! 付款冲正 HTTP 协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::PaymentReversalView;
use erp_returns::dto::{
    CancelPaymentReversalApprovalRequest, CommitPaymentReversalRequest, CreatePaymentReversalRequest,
    PostPaymentReversalRequest, SubmitPaymentReversalRequest,
};
use erp_returns::service::ReturnsService;
use erp_reverse_flow::reverse_flow::ReturnsProcess;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::local_reverse_flow_ports;

/// 查询付款冲正详情，将认证操作人与路径对象交给读模型校验。
/// 返回冲正单视图。
pub async fn payment_reversal_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PaymentReversalView> {
    let view = ReturnsReadService::new(state.application().db(), state.application().authorization())
        .payment_reversal_detail(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记付款冲正草稿（冲正单号唯一构成幂等去重；经办/复核分离）。
/// 返回新建冲正单视图。
pub async fn payment_reversal_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePaymentReversalRequest>,
) -> Result<PaymentReversalView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .create_payment_reversal(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按原付款一次创建付款冲正并启动审批。
/// `req` 为业务命令与期望版本。
pub async fn payment_reversal_commit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitPaymentReversalRequest>,
) -> Result<PaymentReversalView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .commit_payment_reversal(req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 提交付款冲正并启动统一审批。客户端不得选择定义或审批人。
/// `req` 为提交请求（版本与幂等键）。
/// 返回提交后的冲正单视图。
pub async fn payment_reversal_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitPaymentReversalRequest>,
) -> Result<PaymentReversalView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .submit_payment_reversal(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 撤回尚未最终通过的付款冲正审批。
/// `req` 为撤回请求（原因必填）。
/// 返回撤回后的冲正单视图。
pub async fn payment_reversal_cancel_approval(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelPaymentReversalApprovalRequest>,
) -> Result<PaymentReversalView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .cancel_payment_reversal_approval(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 旧付款冲正直接过账入口始终返回冲突，不据其请求形成资金事实；只能由审批最终通过动作触发过账。
pub async fn payment_reversal_post(
    State(_state): State<HttpState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostPaymentReversalRequest>,
) -> Result<PaymentReversalView> {
    match ReturnsService::reject_payment_reversal_client_post() {
        Err(error) => Err(error.into()),
        Ok(never) => match never {},
    }
}
