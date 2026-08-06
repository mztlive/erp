//! 域 D31 `mall_backfill` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::mall_backfill` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    mall_backfill::{
        BackfillCommandRequest, BackfillCommandResultView, BackfillItemListParams, BackfillItemView,
        BackfillJobDetailView, BackfillJobListParams, BackfillJobView, CreateBackfillJobRequest,
        MallBackfillService, PageView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "历史消费回填",
    group_desc = "历史消费回填任务管理与执行",
    desc = "查询历史消费回填任务列表",
    resource = "mall_consumption_backfill_job",
    action = "list"
)]
/// 查询历史消费回填任务列表（W30 任务列表）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`mall_id`/`status`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn backfill_job_list(
    State(state): State<AppState>,
    Query(params): Query<BackfillJobListParams>,
) -> Result<PageView<BackfillJobView>> {
    let page = MallBackfillService::new(state.db())
        .backfill_job_list(&params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "历史消费回填",
    group_desc = "历史消费回填任务管理与执行",
    desc = "创建历史消费回填任务",
    resource = "mall_consumption_backfill_job",
    action = "create"
)]
/// 创建历史消费回填任务草稿（范围终点必须等于切换 `T`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建任务视图。
pub async fn backfill_job_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateBackfillJobRequest>,
) -> Result<BackfillJobView> {
    let view = MallBackfillService::new(state.db())
        .create_backfill_job(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "历史消费回填",
    group_desc = "历史消费回填任务管理与执行",
    desc = "查询历史消费回填任务详情",
    resource = "mall_consumption_backfill_job",
    action = "detail"
)]
/// 查询历史消费回填任务详情（含明细总笔数）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 任务 ID
///
/// # 返回
/// 返回任务详情视图。
pub async fn backfill_job_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<BackfillJobDetailView> {
    let view = MallBackfillService::new(state.db())
        .backfill_job_detail(&id)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "历史消费回填",
    group_desc = "历史消费回填任务管理与执行",
    desc = "执行历史消费回填命令",
    resource = "mall_consumption_backfill_job",
    action = "submit"
)]
/// 执行历史消费回填命令（`START`/`RESUME`，乐观锁 + 幂等键）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 任务 ID
/// * `req` - 命令请求
///
/// # 返回
/// 返回命令结果视图。
pub async fn backfill_job_command(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<BackfillCommandRequest>,
) -> Result<BackfillCommandResultView> {
    let view = MallBackfillService::new(state.db())
        .submit_backfill_command(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "历史消费回填",
    group_desc = "历史消费回填任务管理与执行",
    desc = "查询历史消费回填明细列表",
    resource = "mall_consumption_backfill_item",
    action = "list"
)]
/// 查询历史消费回填明细列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`job_id`/`result`/`cost_basis`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn backfill_item_list(
    State(state): State<AppState>,
    Query(params): Query<BackfillItemListParams>,
) -> Result<PageView<BackfillItemView>> {
    let page = MallBackfillService::new(state.db())
        .backfill_item_list(&params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}
