//! 供应商退款 HTTP 协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::SupplierRefundView;
use erp_returns::dto::{
    CancelSupplierRefundApprovalRequest, CommitSupplierRefundRequest, CreateSupplierRefundRequest,
    PostSupplierRefundRequest, SubmitSupplierRefundRequest,
};
use erp_returns::service::ReturnsService;
use erp_reverse_flow::reverse_flow::ReturnsProcess;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::local_reverse_flow_ports;

/// 查询供应商退款详情，将认证操作人与路径对象交给读模型校验。
/// 返回退款单视图。
pub async fn supplier_refund_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SupplierRefundView> {
    let view = ReturnsReadService::new(state.application().db(), state.application().authorization())
        .supplier_refund_detail(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记供应商退款草稿（退款单号唯一构成幂等去重；经办/复核分离）。
/// 返回新建退款单视图。
pub async fn supplier_refund_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .create_supplier_refund(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按原付款一次创建供应商退款并启动审批。
/// `req` 为业务命令与期望版本。
pub async fn supplier_refund_commit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .commit_supplier_refund(req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 提交供应商退款并启动统一审批。客户端不得选择定义或审批人。
/// `req` 为提交请求（版本与幂等键）。
/// 返回提交后的退款单视图。
pub async fn supplier_refund_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .submit_supplier_refund(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 撤回尚未最终通过的供应商退款审批。
/// `req` 为撤回请求（原因必填）。
/// 返回撤回后的退款单视图。
pub async fn supplier_refund_cancel_approval(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelSupplierRefundApprovalRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .cancel_supplier_refund_approval(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 旧供应商退款过账入口始终返回冲突，不允许用 HTTP 表单生成资金事实；实际过账仅由审批最终通过动作触发。
pub async fn supplier_refund_post(
    State(_state): State<HttpState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    match ReturnsService::reject_supplier_refund_client_post() {
        Err(error) => Err(error.into()),
        Ok(never) => match never {},
    }
}
