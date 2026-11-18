//! 回款凭证按整单详情或原登记人的编辑资格访问。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_finance_posting::finance_posting::receivable::ReceivableProcess;
use erp_read_models::finance::document_files::{FinancialFileView, matching_file};

use crate::http::errors::{Error, Result};
use crate::http::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::{funds_access, receivable_posting_ports};

// 为回款附件列表及下载取得同一份授权清单：草稿走原登记编辑资格，其他读取走整单详情范围。
async fn files(
    state: &HttpState,
    actor: &AuditActor,
    id: &str,
    draft: bool,
) -> std::result::Result<Vec<FinancialFileView>, Error> {
    if draft {
        Ok(ReceivableProcess::new(
            state.application().db(),
            receivable_posting_ports(state.application().db(), state.application().authorization()),
        )
        .customer_receipt_draft_files(id, actor)
        .await?)
    } else {
        Ok(funds_access(state.application().db(), state.application().authorization())
            .customer_receipt_files_scoped(id, actor)
            .await?)
    }
}
// 先按当前入口授权及原单关联定位文件，再读取对象；交付私有响应前重新核对原单关联与资产版本，防止越权或读到已变更的文件。
async fn download(
    state: &HttpState,
    actor: &AuditActor,
    id: &str,
    asset_id: &str,
    draft: bool,
) -> std::result::Result<Response, Error> {
    let rows = files(state, actor, id, draft).await?;
    let file = matching_file(&rows, id, Some(asset_id))?;
    let (view, content) = read_asset(state, actor, &file.file_asset_id).await?;
    matching_file(&files(state, actor, id, draft).await?, id, Some(asset_id))?;
    revalidate_asset(state, &view).await?;
    asset_download_response(&view, content)
}

/// 打开回款详情时按整单读取范围列出该单据的凭证元数据；无权或读取失败时拒绝。
pub async fn receipt_files(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FinancialFileView>> {
    Ok(ApiResponse::ok_with_data(files(&state, &actor, &id, false).await?))
}

/// 从回款详情下载指定凭证。读取对象前后复验详情权限、凭证归属及资产版本，不交付跨单文件。
pub async fn receipt_file_download(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, asset_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    download(&state, &actor, &id, &asset_id, false).await
}

/// 编辑原回款草稿时按当前编辑资格列出凭证，而不是用整单查看资格替代。
pub async fn receipt_draft_files(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FinancialFileView>> {
    Ok(ApiResponse::ok_with_data(files(&state, &actor, &id, true).await?))
}

/// 从回款草稿编辑页私有下载凭证，读取前后核验编辑资格、原单文件关联与资产版本。
pub async fn receipt_draft_file_download(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, asset_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    download(&state, &actor, &id, &asset_id, true).await
}
