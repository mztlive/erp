//! 域 D34 `integration_ops` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::integration_ops` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 权限键 `resource` 用域内对象单数名；人工动作只暴露 W29 强命令，责任迁移和
//! 关闭由 W02 责任 API 承担。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    integration_ops::{
        CreateDifferenceRequest, CreateErrorTaskRequest, DifferenceDetailView, DifferenceListParams,
        DifferenceView, DirectReconciliationCommand, DirectReconciliationResult, ErrorTaskDetailView,
        ErrorTaskListParams, ErrorTaskView, InboxMessageListParams, InboxMessageListView, InboxMessageView,
        IntegrationOpsService, IntegrationTaskActionCommand, IntegrationTaskActionResult,
        IntegrationTaskCompletionCommand, IntegrationTaskCompletionResult, PageView,
        RegisterInboxMessageRequest, WriteBackInboxResultRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "查询入站消息列表",
    resource = "inbox_message",
    action = "list"
)]
/// 查询入站消息列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn inbox_message_list(
    State(state): State<AppState>,
    Query(params): Query<InboxMessageListParams>,
) -> Result<PageView<InboxMessageListView>> {
    let page = IntegrationOpsService::new(state.db())
        .inbox_message_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "查询入站消息详情",
    resource = "inbox_message",
    action = "detail"
)]
/// 查询入站消息详情（含规范化内容引用）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 消息 ID
///
/// # 返回
/// 返回消息详情视图。
pub async fn inbox_message_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<InboxMessageView> {
    let view = IntegrationOpsService::new(state.db())
        .inbox_message_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "登记入站消息",
    resource = "inbox_message",
    action = "register"
)]
/// 登记入站消息（消息层与业务事实层幂等由唯一索引保证，重复投递返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 登记请求
///
/// # 返回
/// 返回新建消息的详情视图。
pub async fn inbox_message_register(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterInboxMessageRequest>,
) -> Result<InboxMessageView> {
    let view = IntegrationOpsService::new(state.db())
        .register_inbox_message(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "回写入站消息处理结果",
    resource = "inbox_message",
    action = "writeback"
)]
/// 回写入站消息处理结果（`processed` 标记已处理；`failed` 同事务登记错误任务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 消息 ID
/// * `req` - 回写请求（含期望版本）
///
/// # 返回
/// 返回回写后的消息详情视图。
pub async fn inbox_message_write_back(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<WriteBackInboxResultRequest>,
) -> Result<InboxMessageView> {
    let view = IntegrationOpsService::new(state.db())
        .write_back_inbox_result(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "查询集成错误任务列表",
    resource = "integration_error_task",
    action = "list"
)]
/// 查询集成错误任务列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn error_task_list(
    State(state): State<AppState>,
    Query(params): Query<ErrorTaskListParams>,
) -> Result<PageView<ErrorTaskView>> {
    let page = IntegrationOpsService::new(state.db())
        .error_task_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "查询集成错误任务详情",
    resource = "integration_error_task",
    action = "detail"
)]
/// 查询集成错误任务详情（含解决/关闭证据文本）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 任务 ID
///
/// # 返回
/// 返回任务详情视图。
pub async fn error_task_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ErrorTaskDetailView> {
    let view = IntegrationOpsService::new(state.db())
        .error_task_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "登记集成错误任务",
    resource = "integration_error_task",
    action = "create"
)]
/// 登记集成错误任务（同一消息与错误分类只允许一个进行中任务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 登记请求
///
/// # 返回
/// 返回新建任务的视图。
pub async fn error_task_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateErrorTaskRequest>,
) -> Result<ErrorTaskView> {
    let view = IntegrationOpsService::new(state.db())
        .create_error_task(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "执行集成任务非终结动作",
    resource = "integration_task",
    action = "process"
)]
/// 执行 W29 非终结任务动作；任务保持 `OPEN`。
pub async fn integration_task_action(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<IntegrationTaskActionCommand>,
) -> Result<IntegrationTaskActionResult> {
    let result = IntegrationOpsService::new(state.db())
        .apply_task_action(command, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "根据已验证结果完成集成任务",
    resource = "integration_task",
    action = "complete"
)]
/// 以 W29 强命令形成领域结论并完成正式任务。
pub async fn integration_task_completion(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<IntegrationTaskCompletionCommand>,
) -> Result<IntegrationTaskCompletionResult> {
    let result = IntegrationOpsService::new(state.db())
        .complete_task(command, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "查询对账差异列表",
    resource = "reconciliation_difference",
    action = "list"
)]
/// 查询对账差异列表（`status` 由最新处理记录派生）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn difference_list(
    State(state): State<AppState>,
    Query(params): Query<DifferenceListParams>,
) -> Result<PageView<DifferenceView>> {
    let page = IntegrationOpsService::new(state.db())
        .difference_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "查询对账差异详情",
    resource = "reconciliation_difference",
    action = "detail"
)]
/// 查询对账差异详情（含处理记录时间线）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 差异 ID
///
/// # 返回
/// 返回差异详情视图。
pub async fn difference_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<DifferenceDetailView> {
    let view = IntegrationOpsService::new(state.db())
        .difference_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "登记对账差异",
    resource = "reconciliation_difference",
    action = "create"
)]
/// 登记对账差异（正式差异事实，创建后不可修改；重复登记返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 登记请求
///
/// # 返回
/// 返回新建差异的视图。
pub async fn difference_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDifferenceRequest>,
) -> Result<DifferenceView> {
    let view = IntegrationOpsService::new(state.db())
        .create_difference(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "提交无正式任务的对账差异决定",
    resource = "reconciliation_difference",
    action = "decide"
)]
/// 对未关联正式任务的差异提交 decision-only 命令。
pub async fn difference_decision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<DirectReconciliationCommand>,
) -> Result<DirectReconciliationResult> {
    let result = IntegrationOpsService::new(state.db())
        .decide_difference(&id, command, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}
