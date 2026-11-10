//! 财务驳回原单的草稿编辑 HTTP 适配；沿用本单 submit 静态资格。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_finance::dto::receivable::{SaveCustomerReceiptDraftRequest, UpdateCustomerReceiptRequest};
use erp_finance_posting::finance_posting::receivable::ReceivableProcess;
use erp_read_models::finance::dto::CustomerReceiptView;

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::receivable_posting_ports;

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
pub async fn customer_receipt_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    let view =
        ReceivableProcess::new(state.application().db(), receivable_posting_ports(state.application().db()))
            .update_customer_receipt(&id, req, &actor)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 保存到账字段与拟核销分配，不启动审批。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 当前操作人。
/// * `req` - 草稿与保存操作号。
/// # 返回
/// 返回已保存原单的完整视图。
/// # 错误
/// 无权限、来源不合法、状态或版本冲突时拒绝。
pub async fn customer_receipt_save_draft(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<SaveCustomerReceiptDraftRequest>,
) -> Result<CustomerReceiptView> {
    let view =
        ReceivableProcess::new(state.application().db(), receivable_posting_ports(state.application().db()))
            .save_customer_receipt_draft(req, &actor)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}
