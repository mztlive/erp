//! 财务驳回原单的草稿编辑 HTTP 适配；沿用本单 submit 静态资格。

use application_core::AuditActor;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use erp_processes::reverse_flow::ReturnsProcess;
use erp_read_models::returns_center::dto::{
    CustomerRefundView, PaymentReversalView, ReceiptReversalView, SupplierRefundView,
};
use erp_returns::dto::UpdateFinancialReturnRequest;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "修改原财务单据草稿",
    resource = "customer_refund",
    action = "submit"
)]
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
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateFinancialReturnRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .update_customer_refund(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "修改原财务单据草稿",
    resource = "supplier_refund",
    action = "submit"
)]
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
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateFinancialReturnRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .update_supplier_refund(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "修改原财务单据草稿",
    resource = "receipt_reversal",
    action = "submit"
)]
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
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateFinancialReturnRequest>,
) -> Result<ReceiptReversalView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .update_receipt_reversal(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "修改原财务单据草稿",
    resource = "payment_reversal",
    action = "submit"
)]
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
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateFinancialReturnRequest>,
) -> Result<PaymentReversalView> {
    let view = ReturnsProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .update_payment_reversal(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
