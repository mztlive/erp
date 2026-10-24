//! 按批准申请登记实际付款，沿原命令身份查证未知提交结果。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, Path, State};
use erp_finance::dto::payable::{
    CommitSupplierPaymentRequest, PaymentRecipientRevealView, RevealPaymentRecipientRequest,
    SupplierPaymentView,
};
use erp_processes::attachments::PendingFileAssets;
use erp_processes::finance_posting::payable::PayableProcess;
use erp_support::SensitivityClass;

use super::process;
use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::extractor::Json;
use crate::core::handler::file_asset::{
    extract_command_with_asset_files, finish_asset_command, store_pending_asset_files,
};
use crate::core::handler::payable::validate_bank_receipt_upload;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "在批准采购付款申请内登记付款",
    resource = "purchase_payment_request",
    action = "pay"
)]
/// 原子登记付款、回单、逐行核销、占用扣减、任务进度及命令结果。
/// # 参数
/// 当前出纳、批准申请身份及带银行回单的付款命令。
/// # 返回
/// 已过账付款单；详情读取失败仍保留已提交事实与回单。
/// # 错误
/// 无执行资格、账户或来源变化、金额越界时拒绝；未知结果保持原命令身份。
pub async fn pay(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<SupplierPaymentView> {
    let (req, files) =
        extract_command_with_asset_files::<CommitSupplierPaymentRequest>(&mut multipart).await?;
    if req.payment_request_id != id {
        return Err(Error::BadRequest("付款申请身份与付款命令不一致".into()));
    }
    // 已命中成功回执时可沿原完整载荷无文件恢复；未命中时新付款由 Process 拒绝空批次。
    // 空批次拒绝不证明先前未知命令未提交；正常未知恢复仍重送原命令和文件。
    if !files.is_empty() {
        validate_bank_receipt_upload(&req, &files)?;
    }
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let result = async {
        let batch = PendingFileAssets::prepare(pending.clone(), &actor)?.shared();
        PayableProcess::new(state.db())
            .with_rbac(state.rbac())
            .with_object_read(state.approval_object_read())
            .commit_purchase_payment_request_with_assets(&id, req, batch, &actor)
            .await
    }
    .await;
    let result = finish_asset_command(&state, &pending, result, |result| result.assets_committed).await?;
    Ok(ApiResponse::ok_with_data(result.view?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "主动查看当前付款申请完整收款账号",
    resource = "party_bank_account",
    action = "reveal"
)]
/// 仅在敏感揭示权限、当前任务和冻结账户版本全部有效时交付完整账号。
/// # 参数
/// 当前账号、申请身份、任务及账户所见版本。
/// # 返回
/// 完整账号；每次成功读取记录敏感审计。
/// # 错误
/// 任务责任、账户归属、状态或版本变化时拒绝。
pub async fn reveal_account(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RevealPaymentRecipientRequest>,
) -> Result<PaymentRecipientRevealView> {
    let codec = state.sensitive_data();
    Ok(ApiResponse::ok_with_data(process(&state).reveal_account(&id, req, &actor, codec.as_ref()).await?))
}
