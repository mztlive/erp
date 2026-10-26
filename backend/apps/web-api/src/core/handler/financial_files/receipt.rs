//! 回款凭证按整单详情或原登记人的编辑资格访问。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_processes::adapters::funds_access_with_rbac;
use erp_processes::finance_posting::receivable::ReceivableProcess;
use erp_read_models::finance::document_files::{FinancialFileView, matching_file};

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};
use crate::core::response::ApiResponse;

/// 编辑入口与查看入口分别核验各自权限，下载前后均重新读取。
async fn files(
    state: &AppState,
    actor: &AuditActor,
    id: &str,
    draft: bool,
) -> std::result::Result<Vec<FinancialFileView>, Error> {
    if draft {
        Ok(ReceivableProcess::new(state.db())
            .with_object_read(state.approval_object_read())
            .customer_receipt_draft_files(id, actor)
            .await?)
    } else {
        Ok(funds_access_with_rbac(state.db(), state.rbac()).customer_receipt_files_scoped(id, actor).await?)
    }
}
/// 文件下载沿用业务资格与前后资产版本重验。
async fn download(
    state: &AppState,
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

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款与销项发票管理",
    desc = "查询回款凭证",
    resource = "customer_receipt",
    action = "detail"
)]
/// 查询回款凭证，保持原单与当前操作人授权。
/// # 参数
/// 应用状态、认证账号和路径中的单据及文件身份。
/// # 返回
/// 授权后的文件清单或私有下载响应。
/// # 错误
/// 无权、跨单文件或读取失败时拒绝。
pub async fn receipt_files(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FinancialFileView>> {
    Ok(ApiResponse::ok_with_data(files(&state, &actor, &id, false).await?))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款与销项发票管理",
    desc = "下载回款凭证",
    resource = "customer_receipt",
    action = "detail"
)]
/// 下载回款凭证，保持原单与当前操作人授权。
/// # 参数
/// 应用状态、认证账号和路径中的单据及文件身份。
/// # 返回
/// 授权后的文件清单或私有下载响应。
/// # 错误
/// 无权、跨单文件或读取失败时拒绝。
pub async fn receipt_file_download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, asset_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    download(&state, &actor, &id, &asset_id, false).await
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款与销项发票管理",
    desc = "查询回款凭证",
    resource = "customer_receipt",
    action = "submit"
)]
/// 查询回款凭证，保持原单与当前操作人授权。
/// # 参数
/// 应用状态、认证账号和路径中的单据及文件身份。
/// # 返回
/// 授权后的文件清单或私有下载响应。
/// # 错误
/// 无权、跨单文件或读取失败时拒绝。
pub async fn receipt_draft_files(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FinancialFileView>> {
    Ok(ApiResponse::ok_with_data(files(&state, &actor, &id, true).await?))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款与销项发票管理",
    desc = "下载回款凭证",
    resource = "customer_receipt",
    action = "submit"
)]
/// 下载回款凭证，保持原单与当前操作人授权。
/// # 参数
/// 应用状态、认证账号和路径中的单据及文件身份。
/// # 返回
/// 授权后的文件清单或私有下载响应。
/// # 错误
/// 无权、跨单文件或读取失败时拒绝。
pub async fn receipt_draft_file_download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, asset_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    download(&state, &actor, &id, &asset_id, true).await
}
