//! 本申请完整来源及冻结材料读取，不展开无关业务附件。

use application_core::AuditActor;
use axum::Extension;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::response::Response;
use erp_core::ids::FileAssetId;
use erp_read_models::finance::payment_request::{
    PurchasePaymentRequestSubmissionView, PurchasePaymentRequestView,
};
use erp_support::FileAssetView;
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;
use serde::Deserialize;

use super::reads;
use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::extractor::Query;
use crate::core::handler::file_asset::{read_asset, revalidate_asset};
use crate::core::response::ApiResponse;

#[derive(Deserialize)]
pub struct PaymentRequestMaterialQuery {
    work_item_id: Option<String>,
    submission_version: Option<u32>,
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "读取采购付款申请历史提交",
    resource = "purchase_payment_request",
    action = "detail"
)]
/// 按完整历史来源资格读取该轮不可变提交依据。
/// # 参数
/// 当前账号、申请身份与提交版本。
/// # 返回
/// 原提交金额、主体、账户与完整采购销售预览。
/// # 错误
/// 历史不存在、来源范围不符或冻结身份不一致时拒绝。
pub async fn submission(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, version)): Path<(String, u32)>,
) -> Result<PurchasePaymentRequestSubmissionView> {
    Ok(ApiResponse::ok_with_data(reads(&state).submission(&actor, &id, version).await?))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "待办队列与责任处理",
    desc = "读取付款申请任务上下文",
    resource = "work_item",
    action = "detail"
)]
/// 当前审批人或出纳凭本任务读取全部冻结采购和销售资料。
/// # 参数
/// 当前账号与正式任务身份。
/// # 返回
/// 本申请完整材料与当前进度。
/// # 错误
/// 任务失效、跨单或无责任资格时拒绝。
pub async fn task_context(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchasePaymentRequestView> {
    Ok(ApiResponse::ok_with_data(
        reads(&state).task_context(&actor, &id, &state.work_item_authorization()).await?,
    ))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "下载本付款申请冻结材料",
    resource = "purchase_payment_request",
    action = "detail"
)]
/// 下载本申请允许材料，在存储读取前后复验完整来源及治理状态。
/// # 参数
/// 当前账号、申请和文件身份；可附当前正式任务身份。
/// # 返回
/// 禁止缓存的私有文件字节。
/// # 错误
/// 关系不符、资格失效或文件变化时拒绝。
pub async fn material_download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, asset_id)): Path<(String, String)>,
    Query(query): Query<PaymentRequestMaterialQuery>,
) -> std::result::Result<Response, Error> {
    content(&state, &actor, &id, &asset_id, &query, true).await
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "预览本付款申请冻结材料",
    resource = "purchase_payment_request",
    action = "detail"
)]
/// 预览允许清单中的同版本图片或 PDF。
/// # 参数
/// 当前账号、申请和文件身份；可附当前正式任务身份。
/// # 返回
/// 禁止缓存和类型嗅探的内联文件。
/// # 错误
/// 文件非安全预览类型、来源不符或资格失效时拒绝。
pub async fn material_preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, asset_id)): Path<(String, String)>,
    Query(query): Query<PaymentRequestMaterialQuery>,
) -> std::result::Result<Response, Error> {
    content(&state, &actor, &id, &asset_id, &query, false).await
}

async fn content(
    state: &AppState,
    actor: &AuditActor,
    id: &str,
    asset_id: &str,
    query: &PaymentRequestMaterialQuery,
    download: bool,
) -> std::result::Result<Response, Error> {
    let reference = material_reference(state, actor, id, asset_id, query).await?;
    if reference.request_id != id {
        return Err(Error::BadRequest("付款申请与当前材料任务不一致".into()));
    }
    let (view, bytes) = read_asset(state, actor, asset_id).await?;
    let actual = ApprovalMaterialFile {
        file_asset_id: FileAssetId::new(&view.id),
        file_name: view.file_name.clone(),
        content_type: view.content_type.clone(),
        byte_size: view.byte_size,
        asset_version: view.version,
        content_hmac: view.content_hmac.clone(),
    };
    if !reference.file.matches_current(&actual) {
        return Err(Error::Conflict("付款申请附件版本已变化，请联系经办人".into()));
    }
    let current = material_reference(state, actor, id, asset_id, query).await?;
    if reference != current {
        return Err(Error::Conflict("付款申请材料资格已变化，请刷新".into()));
    }
    revalidate_asset(state, &view).await?;
    material_response(&view, bytes, download)
}

async fn material_reference(
    state: &AppState,
    actor: &AuditActor,
    id: &str,
    asset_id: &str,
    query: &PaymentRequestMaterialQuery,
) -> std::result::Result<erp_read_models::finance::payment_request::PaymentRequestMaterialReference, Error> {
    let service = reads(state);
    if query.work_item_id.is_some() && query.submission_version.is_some() {
        return Err(Error::BadRequest("任务材料与历史提交材料须分别读取".into()));
    }
    Ok(if let Some(task_id) = query.work_item_id.as_deref() {
        service.require_task_material(actor, task_id, asset_id, &state.work_item_authorization()).await?
    } else if let Some(version) = query.submission_version {
        service.require_submission_material(actor, id, version, asset_id).await?
    } else {
        service.require_material(actor, id, asset_id).await?
    })
}

fn material_response(
    view: &FileAssetView,
    bytes: Vec<u8>,
    download: bool,
) -> std::result::Result<Response, Error> {
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(if download { "application/octet-stream" } else { &view.content_type })
            .map_err(|_| Error::Unprocessable("附件类型无效".into()))?,
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static(if download { "attachment" } else { "inline" }),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}
