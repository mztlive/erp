//! 财务文件沿业务单据或正式任务绑定读取。
pub mod fulfillment_payment;
pub mod invoice;
pub mod receipt;
use std::sync::Arc;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_read_models::finance::document_files::{BusinessFinanceFiles, FinancialFileView, matching_file};

use crate::http::errors::{Error, Result};
use crate::http::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 按本单详情资格读取发票文件，不要求完整财务详情权限。
/// 来源销售单及当前认证账号。
/// 本单发票附件安全展示字段。
/// 来源不可见时拒绝。
pub async fn sales_invoice_files(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FinancialFileView>> {
    Ok(ApiResponse::ok_with_data(
        BusinessFinanceFiles::new(state.application().db(), state.application().authorization())
            .sales(&actor, &id)
            .await?,
    ))
}

/// 下载前后沿销售来源与发票附件双键重验。
/// 来源销售单、所属发票和文件身份。
/// 私有不可缓存文件响应。
/// 跨单文件、资产变化或存储失败时拒绝。
pub async fn sales_invoice_download(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, invoice_id, asset_id)): Path<(String, String, String)>,
) -> std::result::Result<Response, Error> {
    let service = BusinessFinanceFiles::new(state.application().db(), state.application().authorization());
    let files = service.sales(&actor, &id).await?;
    let file = matching_file(&files, &invoice_id, Some(&asset_id))?;
    let (view, content) = read_asset(&state, &actor, &file.file_asset_id).await?;
    matching_file(&service.sales(&actor, &id).await?, &invoice_id, Some(&asset_id))?;
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, content)
}

/// 正式履约任务读取资格覆盖其采购来源付款回单。
/// 本采购来源全部付款回单。
/// 任务不可读或不是采购履约对象时拒绝。
pub async fn purchase_payment_receipts(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FinancialFileView>> {
    Ok(ApiResponse::ok_with_data(task_files(&state, &actor, &id).await?))
}

/// 下载回单前后重验正式任务及精确付款关系。
/// 工作台任务及来源已付款单。
/// 私有文件下载响应。
/// 无任务资格、跨单付款或文件不可读时拒绝。
pub async fn purchase_payment_receipt_download(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, payment_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    let files = task_files(&state, &actor, &id).await?;
    let file = matching_file(&files, &payment_id, None)?;
    let (view, content) = read_asset(&state, &actor, &file.file_asset_id).await?;
    matching_file(&task_files(&state, &actor, &id).await?, &payment_id, Some(&view.id))?;
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, content)
}

// 用工作流授权解析出的采购来源读取任务附件；不接受客户端传入采购单作为授权来源。
async fn task_files(
    state: &HttpState,
    actor: &AuditActor,
    id: &str,
) -> std::result::Result<Vec<FinancialFileView>, Error> {
    BusinessFinanceFiles::new(state.application().db(), state.application().authorization())
        .fulfillment(actor, id, Arc::new(state.application().work_item_authorization()))
        .await
        .map_err(Error::from)
}
