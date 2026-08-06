//! 域 D34 `integration_ops` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::integration_ops` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 权限键 `resource` 用域内对象单数名，`action` 取固定动词表（list/detail/register/
//! writeback/create/query/replay/hold/transfer/resolve/close/process）。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    integration_ops::{
        CloseErrorTaskRequest, CreateDifferenceRequest, CreateErrorTaskRequest, DifferenceActionView,
        DifferenceDetailView, DifferenceListParams, DifferenceView, ErrorTaskDetailView, ErrorTaskListParams,
        ErrorTaskView, HoldErrorTaskRequest, InboxMessageListParams, InboxMessageListView, InboxMessageView,
        IntegrationOpsService, PageView, ProcessDifferenceRequest, QueryOriginalResultRequest,
        RegisterInboxMessageRequest, ReplayOriginalRequest, ReplayResultView, ResolveDifferenceRequest,
        ResolveErrorTaskRequest, TransferErrorTaskRequest, WriteBackInboxResultRequest,
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
    desc = "查询原结果",
    resource = "integration_error_task",
    action = "query"
)]
/// 查询原结果（结果未知任务的 REPLAY 前置动作；任务保持非终结状态）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 任务 ID
/// * `req` - 查询请求（含期望版本与查询结果）
///
/// # 返回
/// 返回查询后的任务视图。
pub async fn error_task_query(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<QueryOriginalResultRequest>,
) -> Result<ErrorTaskView> {
    let view = IntegrationOpsService::new(state.db())
        .query_error_task_result(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "重放原动作",
    resource = "integration_error_task",
    action = "replay"
)]
/// 重放原动作（服务端锁定原幂等键，不接受客户端传入原键；任务保持非终结状态）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 任务 ID
/// * `req` - 重放请求（含期望版本；DTO 拒绝未知字段）
///
/// # 返回
/// 返回重放结果视图（锁定原键摘要 + 锁定标识）。
pub async fn error_task_replay(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReplayOriginalRequest>,
) -> Result<ReplayResultView> {
    let view = IntegrationOpsService::new(state.db())
        .replay_error_task(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "暂挂或跳过当前任务",
    resource = "integration_error_task",
    action = "hold"
)]
/// 暂挂/跳过当前任务（只追加尝试摘要与审计，任务保留在开放队列）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 任务 ID
/// * `req` - 暂挂/跳过请求（含期望版本）
///
/// # 返回
/// 返回任务视图（状态未变，仍在队列）。
pub async fn error_task_hold(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<HoldErrorTaskRequest>,
) -> Result<ErrorTaskView> {
    let view = IntegrationOpsService::new(state.db())
        .hold_error_task(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "转交集成错误任务",
    resource = "integration_error_task",
    action = "transfer"
)]
/// 转交任务（只更新责任人，任务状态不变；转交不是解决）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 任务 ID
/// * `req` - 转交请求（含期望版本与新责任人）
///
/// # 返回
/// 返回转交后的任务视图。
pub async fn error_task_transfer(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<TransferErrorTaskRequest>,
) -> Result<ErrorTaskView> {
    let view = IntegrationOpsService::new(state.db())
        .transfer_error_task(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "解决集成错误任务",
    resource = "integration_error_task",
    action = "resolve"
)]
/// 解决任务（终态：已解决；必须提供非「关闭」解决方式与终态证据）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 任务 ID
/// * `req` - 解决请求（含期望版本）
///
/// # 返回
/// 返回已解决的任务视图。
pub async fn error_task_resolve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ResolveErrorTaskRequest>,
) -> Result<ErrorTaskView> {
    let view = IntegrationOpsService::new(state.db())
        .resolve_error_task(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "关闭集成错误任务",
    resource = "integration_error_task",
    action = "close"
)]
/// 关闭任务（终态：已关闭；重复关闭必须关联替代任务，结果未知任务禁止通用关闭）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 任务 ID
/// * `req` - 关闭请求（含期望版本）
///
/// # 返回
/// 返回已关闭的任务视图。
pub async fn error_task_close(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CloseErrorTaskRequest>,
) -> Result<ErrorTaskView> {
    let view = IntegrationOpsService::new(state.db())
        .close_error_task(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
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
    desc = "人工处理对账差异",
    resource = "reconciliation_difference",
    action = "process"
)]
/// 人工处理对账差异（领取/处理中/补证，只追加处理记录，不终结差异）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 差异 ID
/// * `req` - 处理请求（含期望版本）
///
/// # 返回
/// 返回追加的处理记录视图。
pub async fn difference_process(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ProcessDifferenceRequest>,
) -> Result<DifferenceActionView> {
    let view = IntegrationOpsService::new(state.db())
        .process_difference(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "集成治理",
    group_desc = "入站消息、错误任务与对账差异（W29）",
    desc = "解决对账差异",
    resource = "reconciliation_difference",
    action = "resolve"
)]
/// 解决对账差异（终态结论：确认无误/确认有效差异；固定原因枚举 + 受控证据）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 差异 ID
/// * `req` - 解决请求（含期望版本）
///
/// # 返回
/// 返回追加的处理记录视图。
pub async fn difference_resolve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ResolveDifferenceRequest>,
) -> Result<DifferenceActionView> {
    let view = IntegrationOpsService::new(state.db())
        .resolve_difference(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
