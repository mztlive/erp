//! 域 D27 `projection` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::projection` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 商城连接器由启动组合根注入，Handler 不选择实现。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    projection::{
        CreateSalesOrderProjectionRequest, CreateSalesOrderProjectionRevisionRequest,
        DeliverProjectionRevisionRequest, PageView, ProcessProjectionDeliveriesRequest,
        ProcessProjectionDeliveriesResult, ProjectionBulkCommandRequest, ProjectionBulkCommandResultView,
        ProjectionDeliveryCommand, ProjectionDeliveryResultView, SalesOrderProjectionDeliveryListParams,
        SalesOrderProjectionDeliveryView, SalesOrderProjectionListItemView, SalesOrderProjectionListParams,
        SalesOrderProjectionRevisionView, SalesOrderProjectionView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "查询执行投影列表",
    resource = "sales_order_projection",
    action = "list"
)]
/// 查询销售单执行投影列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn sales_order_projection_list(
    State(state): State<AppState>,
    Query(params): Query<SalesOrderProjectionListParams>,
) -> Result<PageView<SalesOrderProjectionListItemView>> {
    let page = state.projection_service().projection_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "查询执行投影详情",
    resource = "sales_order_projection",
    action = "detail"
)]
/// 查询执行投影详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 投影 ID
///
/// # 返回
/// 返回投影详情视图。
pub async fn sales_order_projection_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SalesOrderProjectionView> {
    let view = state.projection_service().projection_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "建立执行投影",
    resource = "sales_order_projection",
    action = "create"
)]
/// 建立执行投影（存量单切换的第一份投影版本，phase-2 §8.5.4）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 建立请求（销售单 + 目标商城 + 商城侧标识）
///
/// # 返回
/// 返回新建投影的响应视图。
pub async fn sales_order_projection_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSalesOrderProjectionRequest>,
) -> Result<SalesOrderProjectionView> {
    let view = state.projection_service().create_projection(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "推进执行投影版本",
    resource = "sales_order_projection_revision",
    action = "create"
)]
/// 推进执行投影版本（后续 ERP 销售版本 + 下发记录原子写入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 投影 ID
/// * `req` - 推进请求（商城侧标识）
///
/// # 返回
/// 返回新建投影版本的响应视图。
pub async fn sales_order_projection_revision_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreateSalesOrderProjectionRevisionRequest>,
) -> Result<SalesOrderProjectionRevisionView> {
    let view = state
        .projection_service()
        .create_revision(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "查询投影版本列表",
    resource = "sales_order_projection_revision",
    action = "list"
)]
/// 查询投影版本列表（修订号降序）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 投影 ID
///
/// # 返回
/// 返回投影版本视图列表。
pub async fn sales_order_projection_revision_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Vec<SalesOrderProjectionRevisionView>> {
    let view = state.projection_service().revision_list(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "下发投影版本到目标商城",
    resource = "sales_order_projection_delivery",
    action = "submit"
)]
/// 下发投影版本到目标商城（外部调用在事务之外，结果经 `inbox_message` +
/// `integration_error_task` 承接；商城确认后推进投影确认版本）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 投影 ID
/// * `revision_no` - 修订序号
/// * `req` - 下发请求（含幂等键）
///
/// # 返回
/// 返回下发结果视图。
pub async fn sales_order_projection_delivery_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, revision_no)): Path<(String, u32)>,
    Json(req): Json<DeliverProjectionRevisionRequest>,
) -> Result<ProjectionDeliveryResultView> {
    let view = state
        .projection_service()
        .deliver_revision(&id, revision_no, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "查询投影下发记录列表",
    resource = "sales_order_projection_delivery",
    action = "list"
)]
/// 查询投影下发记录列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`target_mall_id` 等扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn sales_order_projection_delivery_list(
    State(state): State<AppState>,
    Query(params): Query<SalesOrderProjectionDeliveryListParams>,
) -> Result<PageView<SalesOrderProjectionDeliveryView>> {
    let page = state.projection_service().delivery_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "执行投递对象强动作",
    resource = "sales_order_projection_delivery",
    action = "operate"
)]
/// 执行 `QUERY_RESULT / RETRY / ESCALATE` 投递对象动作。
pub async fn sales_order_projection_delivery_action(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(delivery_id): Path<String>,
    Json(command): Json<ProjectionDeliveryCommand>,
) -> Result<ProjectionDeliveryResultView> {
    let result = state
        .projection_service()
        .apply_delivery_command(&delivery_id, command, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "批量执行投递对象强动作",
    resource = "sales_order_projection_delivery",
    action = "operate"
)]
/// 对显式选中的投影执行一次服务端批量命令。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 批量动作、显式投影 ID 和幂等键
///
/// # 返回
/// 返回批次汇总和逐项正式结果。
pub async fn sales_order_projection_delivery_bulk_action(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<ProjectionBulkCommandRequest>,
) -> Result<ProjectionBulkCommandResultView> {
    let result = state
        .projection_service()
        .apply_bulk_delivery_command(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "执行投影",
    group_desc = "销售单执行投影与下发（W23）",
    desc = "受控处理待发送投递",
    resource = "sales_order_projection_delivery",
    action = "process_pending"
)]
/// 有界处理待发送及到期重试投递。
///
/// 该入口供内部调度器或受控运维触发；每条记录仍通过 CAS 取得，不接受任意
/// 投影内容，也不把连接器不可用当作成功。
pub async fn sales_order_projection_delivery_process_pending(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<ProcessProjectionDeliveriesRequest>,
) -> Result<ProcessProjectionDeliveriesResult> {
    let result = state
        .projection_service()
        .process_pending_deliveries(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(result))
}
