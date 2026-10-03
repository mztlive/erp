//! 模板目录、管理员维护和销售领号的 HTTP 适配。

use application_core::AuditActor;
use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::response::Response;
use axum::{Extension, Json};
use erp_contract::dto::template::{
    ApplicationView, ApplyTemplateRequest, ConfigureCounterRequest, CounterView, TemplateListParams,
    TemplateStatusRequest, TemplateView,
};
use erp_contract::entity::template_docx::DOCX_MIME;
use erp_contract::service::template::TemplateDownloadSource;
use erp_contract::{ContractTemplateService, Error as ContractError, PageView};
use tracing::error;

use super::template_upload;
use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "合同模板",
    group_desc = "Word 模板与合同领号",
    desc = "查询合同模板",
    resource = "contract_template",
    action = "list"
)]
/// 查询模板目录。
/// # 参数
/// * `state` - 应用状态。
/// * `params` - 目录分页。
/// # 返回
/// 模板公开视图。
/// # 错误
/// 查询失败。
pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<TemplateListParams>,
) -> Result<PageView<TemplateView>> {
    Ok(ApiResponse::ok_with_data(state.contract_template_service().templates(&params).await?))
}

#[permission_macros::permission(
    group = "合同模板",
    group_desc = "Word 模板与合同领号",
    desc = "维护合同模板与编号规则",
    resource = "contract_template",
    action = "manage"
)]
/// 上传不可变 Word 模板，处理及对象写入位于数据库事务外。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 管理员。
/// * `multipart` - DOCX 及模板命令。
/// # 返回
/// 新模板。
/// # 错误
/// 非 DOCX、模板不兼容、主体冲突或写入失败。
pub async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<TemplateView> {
    let (request, file) = template_upload::extract(&mut multipart).await?;
    // 上传时执行真实编号处理，保证领号后文件可处理；SAMPLE 不占流水。
    ContractTemplateService::render(file.bytes.clone(), format!("{}-S-SAMPLE", request.group.prefix()))
        .await?;
    let service = state.contract_template_service();
    let planned = service.prepare(request, &file.name).await?;
    let object_key = planned.object_key();
    state
        .storage()
        .save_owned_with_content_type(&object_key, file.bytes, Some(DOCX_MIME))
        .await
        .map_err(storage_error)?;
    match service.create(planned, &actor).await {
        Ok(view) => Ok(ApiResponse::ok_with_data(view)),
        Err(error) => {
            if !matches!(error, ContractError::OutcomeUnknown(_)) {
                let _ = state.storage().delete(&object_key).await;
            }
            Err(error.into())
        },
    }
}

#[permission_macros::permission(
    group = "合同模板",
    group_desc = "Word 模板与合同领号",
    desc = "启停合同模板",
    resource = "contract_template",
    action = "manage"
)]
/// 更改模板启停状态。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 管理员。
/// * `id` - 模板。
/// * `request` - 版本及状态。
/// # 返回
/// 更新模板。
/// # 错误
/// 版本冲突或写入失败。
pub async fn status(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<TemplateStatusRequest>,
) -> Result<TemplateView> {
    Ok(ApiResponse::ok_with_data(state.contract_template_service().set_status(&id, request, &actor).await?))
}

#[permission_macros::permission(
    group = "合同模板",
    group_desc = "Word 模板与合同领号",
    desc = "读取年度合同流水",
    resource = "contract_template",
    action = "manage"
)]
/// 读取本年度流水。
/// # 参数
/// * `state` - 应用状态。
/// # 返回
/// 四组流水。
/// # 错误
/// 查询失败。
pub async fn counters(State(state): State<AppState>) -> Result<Vec<CounterView>> {
    Ok(ApiResponse::ok_with_data(state.contract_template_service().counters().await?))
}

#[permission_macros::permission(
    group = "合同模板",
    group_desc = "Word 模板与合同领号",
    desc = "校准年度合同流水",
    resource = "contract_template",
    action = "manage"
)]
/// 只向前校准流水。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 管理员。
/// * `request` - 期望版本及流水。
/// # 返回
/// 新流水。
/// # 错误
/// 回退或版本冲突。
pub async fn configure(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<ConfigureCounterRequest>,
) -> Result<CounterView> {
    Ok(ApiResponse::ok_with_data(state.contract_template_service().configure_counter(request, &actor).await?))
}

#[permission_macros::permission(
    group = "合同申请",
    group_desc = "本人合同领号及下载",
    desc = "查询本人合同申请",
    resource = "contract_application",
    action = "list"
)]
/// 查询本人记录。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 销售。
/// * `params` - 分页。
/// # 返回
/// 本人的合同号。
/// # 错误
/// 查询失败。
pub async fn applications(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<TemplateListParams>,
) -> Result<PageView<ApplicationView>> {
    Ok(ApiResponse::ok_with_data(state.contract_template_service().applications(&params, &actor).await?))
}

#[permission_macros::permission(
    group = "合同申请",
    group_desc = "本人合同领号及下载",
    desc = "申请销售合同编号",
    resource = "contract_application",
    action = "create"
)]
/// 从启用模板领号。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 销售。
/// * `request` - 稳定申请键。
/// # 返回
/// 原子生成的合同号。
/// # 错误
/// 申请内容冲突或流水耗尽。
pub async fn apply(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<ApplyTemplateRequest>,
) -> Result<ApplicationView> {
    Ok(ApiResponse::ok_with_data(state.contract_template_service().apply(request, &actor).await?))
}

#[permission_macros::permission(
    group = "合同申请",
    group_desc = "本人合同领号及下载",
    desc = "下载本人带编号 Word 合同",
    resource = "contract_application",
    action = "download"
)]
/// 独立授权后返回带原编号的 DOCX。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 销售。
/// * `id` - 本人申请。
/// # 返回
/// 私有不可缓存的 DOCX。
/// # 错误
/// 越权、对象读取或 Word 处理失败。
pub async fn download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let source = state.contract_template_service().download_source(&id, &actor).await?;
    word_response(&state, source).await
}

#[permission_macros::permission(
    group = "合同模板",
    group_desc = "Word 模板与合同领号",
    desc = "下载编号样张",
    resource = "contract_template",
    action = "manage"
)]
/// 下载不占流水的样张，用于管理员在 Word 中核对首页排版。
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 管理员。
/// * `id` - 模板。
/// # 返回
/// 带 SAMPLE 编号的 Word。
/// # 错误
/// 模板不存在或读取失败。
pub async fn sample(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let source = state.contract_template_service().sample_source(&id, &actor).await?;
    word_response(&state, source).await
}

async fn word_response(
    state: &AppState,
    source: TemplateDownloadSource,
) -> std::result::Result<Response, Error> {
    let bytes = state.storage().read(&source.object_key).await.map_err(storage_error)?;
    let output = ContractTemplateService::render(bytes, source.contract_no.clone()).await?;
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{}.docx\"", source.contract_no))
        .map_err(|_| Error::Internal("合同编号无法下载".into()))?;
    let mut response = Response::new(Body::from(output));
    response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static(DOCX_MIME));
    response.headers_mut().insert(CONTENT_DISPOSITION, disposition);
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

fn storage_error(error: storage::Error) -> Error {
    error!(error = %error, "Contract template storage operation failed");
    Error::Internal("合同模板文件读取或保存失败，请重试".into())
}
