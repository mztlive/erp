//! 域 D22 `legacy_import` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_import` 的领域 DTO 与 `erp_data_import::import_apply::dto` 的组合响应，禁止重复定义同构类型、禁止直连数据库。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_data_import::import_apply::dto::{
    CompleteImportBusinessConfirmationResult, LegacyImportConfirmationView,
};
use erp_import::{
    ApplyLegacyImportBatchRequest, CompleteImportBusinessConfirmationCommand, CreateLegacyImportBatchRequest,
    CreateLegacyImportConfirmationRequest, ImportExecutionCommand, ImportExecutionResult,
    LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchView,
    LegacyImportConfirmationListParams, LegacyImportRowListParams, LegacyImportRowView, PageView,
};
use erp_read_models::import_center;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询导入批次列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn legacy_import_batch_list(
    State(state): State<HttpState>,
    Query(params): Query<LegacyImportBatchListParams>,
) -> Result<PageView<LegacyImportBatchListItem>> {
    let mut page = state.application().legacy_import_service().batch_list(&params).await?;
    import_center::batch_names(&state.application().db(), &mut page.items).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 创建导入批次（批次 + 来源行 + 后台任务原子写入，批次号重复按幂等返回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（批次头 + 来源行）
///
/// # 返回
/// 返回新建（或既有）批次的响应视图。
pub async fn legacy_import_batch_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateLegacyImportBatchRequest>,
) -> Result<LegacyImportBatchView> {
    let view = state.application().import_apply_process().create_batch(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 查询导入批次详情（含后台任务关联）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 导入批次 ID
///
/// # 返回
/// 返回批次的响应视图。
pub async fn legacy_import_batch_detail(
    State(state): State<HttpState>,
    Path(id): Path<String>,
) -> Result<LegacyImportBatchView> {
    let mut view = state.application().legacy_import_service().batch_detail(&id).await?;
    import_center::batch_name(&state.application().db(), &mut view).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 应用导入批次（后台应用阶段逐行结果，批次/行终态后重复提交按幂等返回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 导入批次 ID
/// * `req` - 逐行结果
///
/// # 返回
/// 返回应用后批次的响应视图。
pub async fn legacy_import_batch_apply(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ApplyLegacyImportBatchRequest>,
) -> Result<LegacyImportBatchView> {
    let view = state.application().import_apply_process().apply_batch(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 执行 W18 导入应用阶段强命令。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 导入批次 ID；必须与命令体 `batch_id` 一致
/// * `command` - `START_APPLY` / `CANCEL_PENDING` / `RETRY_FAILED` 强命令
///
/// # 返回
/// 返回事务提交后的批次、后台任务、下一步与审计收据。
pub async fn legacy_import_execution_command(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ImportExecutionCommand>,
) -> Result<ImportExecutionResult> {
    let result =
        state.application().import_apply_process().execute_import_command(&id, command, &actor).await?;

    Ok(ApiResponse::ok_with_data(result))
}

/// 查询导入行列表（按批次）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 导入批次 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn legacy_import_row_list(
    State(state): State<HttpState>,
    Path(id): Path<String>,
    Query(params): Query<LegacyImportRowListParams>,
) -> Result<PageView<LegacyImportRowView>> {
    let page = state.application().legacy_import_service().row_list(&id, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询导入确认事实列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的当前查询人
/// * `query` - 分页与筛选参数（`batch_id` 为主要筛选）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn legacy_import_confirmation_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<LegacyImportConfirmationListParams>,
) -> Result<PageView<LegacyImportConfirmationView>> {
    let page = state.application().import_apply_process().confirmation_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 创建待确认确认事实（批次推进到待确认阶段，重复提交按幂等返回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建（或既有）确认事实的响应视图。
pub async fn legacy_import_confirmation_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateLegacyImportConfirmationRequest>,
) -> Result<LegacyImportConfirmationView> {
    let view = state.application().import_apply_process().create_confirmation(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 执行 W18 唯一 `CompleteImportBusinessConfirmation` 强类型命令。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 带任务、批次、试算版本和幂等键的强类型命令
///
/// # 返回
/// 返回确认事实、已完成任务、批次新版本、下一步与审计收据。
pub async fn legacy_import_confirmation_complete(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CompleteImportBusinessConfirmationCommand>,
) -> Result<CompleteImportBusinessConfirmationResult> {
    let view =
        state.application().import_apply_process().complete_import_business_confirmation(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}
