//! 财务驳回原单的草稿编辑 HTTP 适配；沿用本单 submit 静态资格。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::returns_center::dto::{
    CustomerRefundView, PaymentReversalView, ReceiptReversalView, SupplierRefundView,
};
use erp_returns::dto::UpdateFinancialReturnRequest;
use erp_reverse_flow::reverse_flow::ReturnsProcess;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::local_reverse_flow_ports;

/// 保存原财务单据的草稿字段；服务端校验原经办人、状态、版本及完整资金源。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前已认证经办人
/// * `id` - 原财务单据主键
/// * `req` - 当前版本与可编辑字段
///
/// # 返回
/// 返回原单当前详情，含保存后的乐观锁版本。
///
/// # 错误
/// 无资格、非原经办人、非草稿、版本变化或非法字段时返回对应错误。
pub async fn customer_refund_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateFinancialReturnRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .update_customer_refund(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 保存原财务单据的草稿字段；服务端校验原经办人、状态、版本及完整资金源。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前已认证经办人
/// * `id` - 原财务单据主键
/// * `req` - 当前版本与可编辑字段
///
/// # 返回
/// 返回原单当前详情，含保存后的乐观锁版本。
///
/// # 错误
/// 无资格、非原经办人、非草稿、版本变化或非法字段时返回对应错误。
pub async fn supplier_refund_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateFinancialReturnRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .update_supplier_refund(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 保存原财务单据的草稿字段；服务端校验原经办人、状态、版本及完整资金源。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前已认证经办人
/// * `id` - 原财务单据主键
/// * `req` - 当前版本与可编辑字段
///
/// # 返回
/// 返回原单当前详情，含保存后的乐观锁版本。
///
/// # 错误
/// 无资格、非原经办人、非草稿、版本变化或非法字段时返回对应错误。
pub async fn receipt_reversal_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateFinancialReturnRequest>,
) -> Result<ReceiptReversalView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .update_receipt_reversal(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 保存原财务单据的草稿字段；服务端校验原经办人、状态、版本及完整资金源。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前已认证经办人
/// * `id` - 原财务单据主键
/// * `req` - 当前版本与可编辑字段
///
/// # 返回
/// 返回原单当前详情，含保存后的乐观锁版本。
///
/// # 错误
/// 无资格、非原经办人、非草稿、版本变化或非法字段时返回对应错误。
pub async fn payment_reversal_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateFinancialReturnRequest>,
) -> Result<PaymentReversalView> {
    let view = ReturnsProcess::new(
        state.application().db(),
        local_reverse_flow_ports(state.application().db(), state.application().authorization()),
    )
    .update_payment_reversal(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}
