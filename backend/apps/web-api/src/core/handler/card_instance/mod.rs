//! 域 D28 `card_instance` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::card_instance` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use entities::ids::MallCardInstanceId;
use services::{
    audit::AuditActor,
    card_instance::{
        BalanceSnapshotListParams, BalanceSnapshotView, CardInstanceDetailView, CardInstanceListParams,
        CardInstanceService, CardInstanceView, CorrectionListParams, CorrectionView,
        CreateBalanceSnapshotRequest, CreateCardInstanceRequest, CreateCutoverRequest, CutoverListParams,
        CutoverView, EnableCutoverRequest, PageView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "查询商城切换记录列表",
    resource = "mall_consumption_cutover",
    action = "list"
)]
/// 查询商城切换记录列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`mall_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn cutover_list(
    State(state): State<AppState>,
    Query(params): Query<CutoverListParams>,
) -> Result<PageView<CutoverView>> {
    let page = CardInstanceService::new(state.db()).cutover_list(&params).await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "创建商城切换记录",
    resource = "mall_consumption_cutover",
    action = "create"
)]
/// 创建商城切换记录（准备态，未启用）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ mall_id, checklist_reference? }`）
///
/// # 返回
/// 返回新建切换记录视图。
pub async fn cutover_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCutoverRequest>,
) -> Result<CutoverView> {
    let view = CardInstanceService::new(state.db())
        .create_cutover(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "启用商城切换（登记唯一 T）",
    resource = "mall_consumption_cutover",
    action = "submit"
)]
/// 启用商城切换（登记唯一 `T`；乐观锁冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 切换记录 ID
/// * `req` - 启用请求（期望版本 + `T`）
///
/// # 返回
/// 返回启用后的切换记录视图。
pub async fn cutover_enable(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<EnableCutoverRequest>,
) -> Result<CutoverView> {
    let view = CardInstanceService::new(state.db())
        .enable_cutover(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "查询商城切换记录详情",
    resource = "mall_consumption_cutover",
    action = "detail"
)]
/// 查询商城切换记录详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 切换记录 ID
///
/// # 返回
/// 返回切换记录视图。
pub async fn cutover_detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<CutoverView> {
    let view = CardInstanceService::new(state.db()).cutover_detail(&id).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "查询卡实例列表",
    resource = "mall_card_instance",
    action = "list"
)]
/// 查询卡实例列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`mall_id`/`opaque_instance_ref`/`source_type`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn card_instance_list(
    State(state): State<AppState>,
    Query(params): Query<CardInstanceListParams>,
) -> Result<PageView<CardInstanceView>> {
    let page = CardInstanceService::new(state.db())
        .card_instance_list(&params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "建立卡实例基线",
    resource = "mall_card_instance",
    action = "create"
)]
/// 建立卡实例基线（基线 + 初始余额快照原子写入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建（或幂等返回既有）卡实例视图。
pub async fn card_instance_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCardInstanceRequest>,
) -> Result<CardInstanceView> {
    let view = CardInstanceService::new(state.db())
        .create_card_instance(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "查询卡实例详情",
    resource = "mall_card_instance",
    action = "detail"
)]
/// 查询卡实例详情（基线 + 最新余额 + 快照/纠错摘要）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 卡实例 ID
///
/// # 返回
/// 返回卡实例详情视图。
pub async fn card_instance_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<CardInstanceDetailView> {
    let view = CardInstanceService::new(state.db())
        .card_instance_detail(&id)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "查询卡实例余额快照列表",
    resource = "mall_balance_snapshot",
    action = "list"
)]
/// 查询卡实例余额快照列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - URI 中必填的卡实例 ID
/// * `query` - 分页与排序参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn balance_snapshot_list(
    State(state): State<AppState>,
    Path(id): Path<MallCardInstanceId>,
    Query(params): Query<BalanceSnapshotListParams>,
) -> Result<PageView<BalanceSnapshotView>> {
    let page = CardInstanceService::new(state.db())
        .balance_snapshot_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "追加卡实例余额快照",
    resource = "mall_balance_snapshot",
    action = "create"
)]
/// 追加卡实例余额快照（商城余额快照回流）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建快照视图。
pub async fn balance_snapshot_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateBalanceSnapshotRequest>,
) -> Result<BalanceSnapshotView> {
    let view = CardInstanceService::new(state.db())
        .create_balance_snapshot(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "卡券消费台账",
    group_desc = "商城消费回流切换、卡实例基线与余额快照管理",
    desc = "查询卡实例纠错列表",
    resource = "mall_card_instance_correction",
    action = "list"
)]
/// 查询卡实例纠错列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - URI 中必填的卡实例 ID
/// * `query` - 分页与排序参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn correction_list(
    State(state): State<AppState>,
    Path(id): Path<MallCardInstanceId>,
    Query(params): Query<CorrectionListParams>,
) -> Result<PageView<CorrectionView>> {
    let page = CardInstanceService::new(state.db())
        .correction_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}
