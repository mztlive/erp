//! 采购内部报价、费用依据和附件使用独立权限及当前任务访问。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_processes::sales_inquiry::dto::InquiryMutationView;
use erp_processes::sales_inquiry::internal_quote::{SaveInquiryFeeBasisCommand, SaveInquiryQuoteCommand};
use erp_processes::sales_inquiry::read::{InquiryFeeBasisParams, InquiryReadService, InquiryTaskParams};
use erp_supply::inquiry::InquiryFeeBasis;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::extractor::{Json, Query};
use crate::core::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "保存询价内部报价",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 追加当前责任商品的内部报价版本，不隐式修改通用供给。
/// # 参数
/// `state`、`actor` 为当前应用及采购，`id` 为询价，`request` 为带版本及任务的报价。
/// # 返回
/// 返回安全询价命令回执。
/// # 错误
/// 任务、权限、供应资格、报价范围或版本不符时拒绝。
pub async fn save_quote(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<SaveInquiryQuoteCommand>,
) -> Result<InquiryMutationView> {
    Ok(ApiResponse::ok_with_data(
        state.sales_inquiry_process().save_internal_quote(&actor, &id, request).await?,
    ))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "保存询价费用内部依据",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 保存明确费用的内部收款依据，公开反馈只保留对销售金额及说明。
/// # 参数
/// `state`、`actor` 为当前应用及采购，`id` 为询价，`request` 为责任任务和费用内部依据。
/// # 返回
/// 返回安全询价命令回执。
/// # 错误
/// 任务、费用范围、供应商或付款条件非法时拒绝。
pub async fn save_fee_basis(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<SaveInquiryFeeBasisCommand>,
) -> Result<InquiryMutationView> {
    Ok(ApiResponse::ok_with_data(state.sales_inquiry_process().save_fee_basis(&actor, &id, request).await?))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "读取询价费用内部依据",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 读取当前责任范围内明确轮次的内部费用依据。
/// # 参数
/// `state`、`actor` 为当前应用及采购，`id` 为询价，`params` 为当前任务及反馈轮次。
/// # 返回
/// 返回责任范围内费用收款资料。
/// # 错误
/// 权限、当前任务、轮次或范围失效时拒绝。
pub async fn fee_bases(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<InquiryFeeBasisParams>,
) -> Result<Vec<InquiryFeeBasis>> {
    Ok(ApiResponse::ok_with_data(
        InquiryReadService::new(state.db(), state.rbac()).fee_bases(&actor, &id, &params).await?,
    ))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "下载当前责任报价附件",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 在存储读取前后复核当前任务、报价及精确附件关系。
/// # 参数
/// `state`、`actor` 为当前应用及采购，路径含询价、报价版本、文件；查询含任务身份。
/// # 返回
/// 返回禁止缓存及嗅探的原附件下载，不公开存储键。
/// # 错误
/// 当前任务、关系、文件治理状态或内容指纹失效时拒绝。
pub async fn quote_attachment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, quote_id, asset_id)): Path<(String, String, String)>,
    Query(params): Query<InquiryTaskParams>,
) -> std::result::Result<Response, Error> {
    let service = InquiryReadService::new(state.db(), state.rbac());
    service.require_quote_attachment(&actor, &id, &params.task_id, &quote_id, &asset_id).await?;
    let (view, bytes) = read_asset(&state, &actor, &asset_id).await?;
    service.require_quote_attachment(&actor, &id, &params.task_id, &quote_id, &asset_id).await?;
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, bytes)
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "下载当前责任费用依据附件",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 在存储读取前后复核当前费用责任、反馈轮次及精确依据附件关系。
/// # 参数
/// `state`、`actor` 为当前应用及采购，路径含询价、依据及文件；查询含任务和反馈轮次。
/// # 返回
/// 返回禁止缓存及嗅探的附件下载，存储键不进入响应。
/// # 错误
/// 当前任务、费用范围、附件关系、文件治理或指纹失效时拒绝。
pub async fn fee_basis_attachment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, basis_id, asset_id)): Path<(String, String, String)>,
    Query(params): Query<InquiryFeeBasisParams>,
) -> std::result::Result<Response, Error> {
    let service = InquiryReadService::new(state.db(), state.rbac());
    service.require_fee_basis_attachment(&actor, &id, &params, &basis_id, &asset_id).await?;
    let (view, bytes) = read_asset(&state, &actor, &asset_id).await?;
    service.require_fee_basis_attachment(&actor, &id, &params, &basis_id, &asset_id).await?;
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, bytes)
}
