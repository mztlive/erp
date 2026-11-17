//! 发票 multipart 边界复用通用上传校验，资产登记加入原发票事务。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, State};
use erp_file_assets::PendingFileAssets;
use erp_finance::dto::receivable::{
    CommitInvoiceRequest, CompleteInvoiceRequest, CreateInvoiceRequest, InvoiceView,
};
use erp_finance_posting::finance_posting::receivable::ReceivableProcess;
use erp_finance_posting::{FinanceAttachmentService, FinanceAttachmentUpload};
use erp_support::SensitivityClass;

use crate::http::errors::Result;
use crate::http::handler::file_asset::{
    delete_pending_asset_objects, extract_command_with_asset_files, finish_asset_command,
    store_pending_asset_files,
};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::receivable_posting_ports;

/// 登记发票草稿同时上传图片或 PDF。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `multipart` - 发票 JSON 命令和按临时引用命名的文件
///
/// # 返回
/// 原发票草稿响应。
///
/// # 错误
/// 文件、业务字段或事务失败时拒绝并按确定回滚补偿存储对象。
pub async fn invoice_create_with_files(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<InvoiceView> {
    FinanceAttachmentService::new(state.application().db(), state.application().authorization())
        .prepare_attachment_upload(&actor, FinanceAttachmentUpload::InvoiceDraft)
        .await?;
    let (req, files) = extract_command_with_asset_files::<CreateInvoiceRequest>(&mut multipart).await?;
    let uploads = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let pending = match PendingFileAssets::prepare(uploads.clone(), &actor) {
        Ok(pending) => pending.shared(),
        Err(error) => {
            delete_pending_asset_objects(&state, &uploads).await;
            return Err(error.into());
        },
    };
    let outcome = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .create_invoice_with_assets(req, pending, &actor)
    .await;
    let view = finish_asset_command(&state, &uploads, outcome, |_| true).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 原子开票时在同一事务登记文件和关联；重放删除本次未消费上传。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `multipart` - 开票 JSON 命令和按临时引用命名的文件
///
/// # 返回
/// 原销项发票响应。
///
/// # 错误
/// 文件或事务失败时拒绝；提交结果未知时保留存储对象。
pub async fn invoice_commit_with_files(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<InvoiceView> {
    FinanceAttachmentService::new(state.application().db(), state.application().authorization())
        .prepare_attachment_upload(&actor, FinanceAttachmentUpload::InvoicePosting)
        .await?;
    let (req, files) = extract_command_with_asset_files::<CommitInvoiceRequest>(&mut multipart).await?;
    let uploads = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let pending = match PendingFileAssets::prepare(uploads.clone(), &actor) {
        Ok(pending) => pending.shared(),
        Err(error) => {
            delete_pending_asset_objects(&state, &uploads).await;
            return Err(error.into());
        },
    };
    let outcome = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .commit_invoice_with_assets(req, pending, &actor)
    .await;
    let result = finish_asset_command(&state, &uploads, outcome, |result| result.assets_committed).await?;
    Ok(ApiResponse::ok_with_data(result.view))
}

/// 按剩余批准金额完成开票并关联附件；重放删除未消费上传。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证账号
/// * `multipart` - 开票 JSON 命令和按临时引用命名的文件
///
/// # 返回
/// 原销项发票响应。
///
/// # 错误
/// 文件或事务失败时拒绝；提交结果未知时保留存储对象。
pub async fn invoice_complete_with_files(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<InvoiceView> {
    FinanceAttachmentService::new(state.application().db(), state.application().authorization())
        .prepare_attachment_upload(&actor, FinanceAttachmentUpload::InvoicePosting)
        .await?;
    let (req, files) = extract_command_with_asset_files::<CompleteInvoiceRequest>(&mut multipart).await?;
    let uploads = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let pending = match PendingFileAssets::prepare(uploads.clone(), &actor) {
        Ok(pending) => pending.shared(),
        Err(error) => {
            delete_pending_asset_objects(&state, &uploads).await;
            return Err(error.into());
        },
    };
    let outcome = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .complete_invoice_with_assets(req, pending, &actor)
    .await;
    let result = finish_asset_command(&state, &uploads, outcome, |result| result.assets_committed).await?;
    Ok(ApiResponse::ok_with_data(result.view))
}
