//! 域 D34 `integration_ops` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_integration::dto` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 权限键 `resource` 用域内对象单数名；人工动作只暴露 W29 强命令，责任迁移和
//! 关闭由 W02 责任 API 承担。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_integration::dto::{
    CreateDifferenceRequest, CreateErrorTaskRequest, DifferenceDetailView, DifferenceListParams,
    DifferenceListView, DifferenceView, DirectReconciliationCommand, DirectReconciliationResult,
    ErrorTaskListParams, ErrorTaskListView, ErrorTaskView, InboxMessageListParams, InboxMessageListView,
    InboxMessageView, IntegrationTaskActionCommand, IntegrationTaskActionResult,
    IntegrationTaskCompletionCommand, IntegrationTaskCompletionResult, PageView, RegisterInboxMessageRequest,
    WriteBackInboxResultRequest,
};
use erp_read_models::integration_center::IntegrationErrorDetailView;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 按请求条件查询入站消息列表。
pub async fn inbox_message_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<InboxMessageListParams>,
) -> Result<PageView<InboxMessageListView>> {
    let page = crate::wiring::scoped_integration_ops_service(
        state.application().db(),
        state.application().authorization(),
    )
    .inbox_message_list(&params, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询入站消息详情（含规范化内容引用）。
/// 返回消息详情视图。
pub async fn inbox_message_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<InboxMessageView> {
    let view = crate::wiring::scoped_integration_ops_service(
        state.application().db(),
        state.application().authorization(),
    )
    .inbox_message_detail(&id, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记入站消息（消息层与业务事实层幂等由唯一索引保证，重复投递返回 409）。
/// 返回新建消息的详情视图。
pub async fn inbox_message_register(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterInboxMessageRequest>,
) -> Result<InboxMessageView> {
    let view = state.application().integration_resolution().register_inbox_message(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 回写入站消息处理结果（`processed` 标记已处理；`failed` 同事务登记错误任务）。
/// `req` 为回写请求（含期望版本）。
/// 返回回写后的消息详情视图。
pub async fn inbox_message_write_back(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<WriteBackInboxResultRequest>,
) -> Result<InboxMessageView> {
    let view = state.application().integration_resolution().write_back_inbox_result(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按请求条件查询集成错误任务列表。
pub async fn error_task_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ErrorTaskListParams>,
) -> Result<ErrorTaskListView> {
    let page = state.application().integration_center().error_task_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询集成错误任务详情（含解决/关闭证据文本）。
/// 返回任务详情视图。
pub async fn error_task_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<IntegrationErrorDetailView> {
    let view = state.application().integration_center().error_task_detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记集成错误任务（同一消息与错误分类只允许一个进行中任务）。
/// 返回新建任务的视图。
pub async fn error_task_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateErrorTaskRequest>,
) -> Result<ErrorTaskView> {
    let view = state.application().integration_resolution().create_error_task(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 在 W29 正式任务办理中执行不终结任务的动作；领域用例返回后，任务仍保持 `OPEN`。
pub async fn integration_task_action(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<IntegrationTaskActionCommand>,
) -> Result<IntegrationTaskActionResult> {
    let result = state.application().integration_resolution().apply_task_action(command, &actor).await?;
    Ok(ApiResponse::ok_with_data(result))
}

/// 完成 W29 正式任务时走强命令，把领域处理结论与任务完成交给同一编排入口。
pub async fn integration_task_completion(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<IntegrationTaskCompletionCommand>,
) -> Result<IntegrationTaskCompletionResult> {
    let result = state.application().integration_resolution().complete_task(command, &actor).await?;
    Ok(ApiResponse::ok_with_data(result))
}

/// 查询对账差异列表（`status` 由最新处理记录派生）。
pub async fn difference_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<DifferenceListParams>,
) -> Result<DifferenceListView> {
    let page = state.application().integration_center().difference_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询对账差异详情（含处理记录时间线）。
/// 返回差异详情视图。
pub async fn difference_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<DifferenceDetailView> {
    let view = state.application().integration_center().difference_detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记对账差异（正式差异事实，创建后不可修改；重复登记返回 409）。
/// 返回新建差异的视图。
pub async fn difference_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDifferenceRequest>,
) -> Result<DifferenceView> {
    let view = state.application().integration_resolution().create_difference(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 只对没有关联正式任务的对账差异办理独立决策，不用该入口替代 W29 任务结案。
pub async fn difference_decision(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<DirectReconciliationCommand>,
) -> Result<DirectReconciliationResult> {
    let result = state.application().integration_resolution().decide_difference(&id, command, &actor).await?;
    Ok(ApiResponse::ok_with_data(result))
}
