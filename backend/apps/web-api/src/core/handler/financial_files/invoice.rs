//! 财务整票附件按发票详情权限与完整数据范围读取。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_processes::adapters::{funds_access_with_rbac, purchase_access};
use erp_read_models::finance::document_files::{FinancialFileView, matching_file};

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};
use crate::core::response::ApiResponse;

/// 复用财务详情授权，文件清单与资格由读取事务同时核验。
async fn files(
    state: &AppState,
    actor: &AuditActor,
    id: &str,
) -> std::result::Result<Vec<FinancialFileView>, Error> {
    Ok(funds_access_with_rbac(state.db(), state.rbac())
        .invoice_files_scoped(id, actor, &purchase_access(state.db(), state.rbac()))
        .await?)
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款与销项发票管理",
    desc = "查询发票文件",
    resource = "invoice",
    action = "detail"
)]
/// 按整票详情资格列出文件。
/// # 参数
/// 应用状态、当前账号及发票身份。
/// # 返回
/// 已授权发票的文件清单。
/// # 错误
/// 无整票权限或读取失败时拒绝。
pub async fn invoice_files(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FinancialFileView>> {
    Ok(ApiResponse::ok_with_data(files(&state, &actor, &id).await?))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款与销项发票管理",
    desc = "下载发票文件",
    resource = "invoice",
    action = "detail"
)]
/// 下载前后重验发票资格、文件关联及文件状态。
/// # 参数
/// 应用状态、当前账号、发票及文件身份。
/// # 返回
/// 私有不可缓存文件响应。
/// # 错误
/// 跨票、权限变化、资产变化或读取失败时拒绝。
pub async fn invoice_download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, asset_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    let rows = files(&state, &actor, &id).await?;
    let file = matching_file(&rows, &id, Some(&asset_id))?;
    let (view, content) = read_asset(&state, &actor, &file.file_asset_id).await?;
    matching_file(&files(&state, &actor, &id).await?, &id, Some(&asset_id))?;
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, content)
}
