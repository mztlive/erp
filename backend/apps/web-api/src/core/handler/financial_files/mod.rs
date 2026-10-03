//! 财务文件沿业务单据或正式任务绑定读取。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_processes::adapters::workflow::work_item_service;
use erp_read_models::finance::document_files::{BusinessFinanceFiles, FinancialFileView, matching_file};
use erp_workflow::WorkItemType;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "下载本单发票文件",
    resource = "sales_order",
    action = "detail"
)]
/// 按本单详情资格读取发票文件，不要求完整财务详情权限。
/// # 参数
/// 来源销售单及当前认证账号。
/// # 返回
/// 本单发票附件安全展示字段。
/// # 错误
/// 来源不可见时拒绝。
pub async fn sales_invoice_files(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FinancialFileView>> {
    Ok(ApiResponse::ok_with_data(
        BusinessFinanceFiles::new(state.db(), state.rbac()).sales(&actor, &id).await?,
    ))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "下载本单发票文件",
    resource = "sales_order",
    action = "detail"
)]
/// 下载前后沿销售来源与发票附件双键重验。
/// # 参数
/// 来源销售单、所属发票和文件身份。
/// # 返回
/// 私有不可缓存文件响应。
/// # 错误
/// 跨单文件、资产变化或存储失败时拒绝。
pub async fn sales_invoice_download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, invoice_id, asset_id)): Path<(String, String, String)>,
) -> std::result::Result<Response, Error> {
    let service = BusinessFinanceFiles::new(state.db(), state.rbac());
    let files = service.sales(&actor, &id).await?;
    let file = matching_file(&files, &invoice_id, Some(&asset_id))?;
    let (view, content) = read_asset(&state, &actor, &file.file_asset_id).await?;
    matching_file(&service.sales(&actor, &id).await?, &invoice_id, Some(&asset_id))?;
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, content)
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "下载采购履约付款回单",
    resource = "work_item",
    action = "detail"
)]
/// 正式履约任务读取资格覆盖其采购来源付款回单。
/// # 参数
/// 正式任务及当前账号。
/// # 返回
/// 本采购来源全部付款回单。
/// # 错误
/// 任务不可读或不是采购履约对象时拒绝。
pub async fn purchase_payment_receipts(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<FinancialFileView>> {
    Ok(ApiResponse::ok_with_data(task_files(&state, &actor, &id).await?))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "下载采购履约付款回单",
    resource = "work_item",
    action = "detail"
)]
/// 下载回单前后重验正式任务及精确付款关系。
/// # 参数
/// 工作台任务及来源已付款单。
/// # 返回
/// 私有文件下载响应。
/// # 错误
/// 无任务资格、跨单付款或文件不可读时拒绝。
pub async fn purchase_payment_receipt_download(
    State(state): State<AppState>,
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

/// 来源单据完全来自工作流授权结果，客户端不得注入采购单。
async fn task_files(
    state: &AppState,
    actor: &AuditActor,
    id: &str,
) -> std::result::Result<Vec<FinancialFileView>, Error> {
    let authorized = work_item_service(state.db(), state.rbac()).authorize_work_item(id, actor).await?;
    if authorized.item.work_item_type != WorkItemType::FulfillmentOperation {
        return Err(Error::NotFound("采购履约任务不存在或无权查看".into()));
    }
    BusinessFinanceFiles::new(state.db(), state.rbac())
        .fulfillment(&authorized.item.business_object_type, &authorized.item.business_object_id)
        .await
        .map_err(Error::from)
}
