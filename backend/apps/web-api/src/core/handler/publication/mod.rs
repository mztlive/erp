//! 域 D26 `publication` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::publication` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 商城连接器（`MallConnector`）在 handler 内构造默认失败关闭实现，
//! 保持 Service 可注入 mock 连接器。

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    publication::{
        CreateProductPublicationRevisionRequest, DeliverPublicationRevisionRequest, PageView,
        ProcessPublicationDeliveriesRequest, ProcessPublicationDeliveriesResult,
        ProductPublicationDeliveryListParams, ProductPublicationDeliveryView, ProductPublicationListParams,
        ProductPublicationRevisionMediaView, ProductPublicationRevisionView, ProductPublicationView,
        PublicationDeliveryActionResultView, PublicationDeliveryCommand, PublicationDeliveryResultView,
        PublicationService, SystemSafetyPauseOperationView, UnavailableMallConnector,
        UpdateProductPublicationRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "查询商品发布列表",
    resource = "product_publication",
    action = "list"
)]
/// 查询商品发布列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn product_publication_list(
    State(state): State<AppState>,
    Query(params): Query<ProductPublicationListParams>,
) -> Result<PageView<ProductPublicationView>> {
    let page = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .publication_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "查询商品发布详情",
    resource = "product_publication",
    action = "detail"
)]
/// 查询商品发布详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 发布 ID
///
/// # 返回
/// 返回发布详情视图。
pub async fn product_publication_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ProductPublicationView> {
    let view = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .publication_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "更新商品发布",
    resource = "product_publication",
    action = "update"
)]
/// 更新商品发布（乐观锁：请求携带期望版本，冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 发布 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后发布的响应视图。
pub async fn product_publication_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductPublicationRequest>,
) -> Result<ProductPublicationView> {
    let view = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .update_publication(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "形成发布修订",
    resource = "product_publication_revision",
    action = "create"
)]
/// 形成发布修订（不可变版本 + 受控媒体原子写入，发布推进为待发布）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 发布 ID
/// * `req` - 形成修订请求（含媒体清单）
///
/// # 返回
/// 返回新建发布修订的响应视图。
pub async fn product_publication_revision_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreateProductPublicationRevisionRequest>,
) -> Result<ProductPublicationRevisionView> {
    let view = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .create_revision(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "查询发布修订列表",
    resource = "product_publication_revision",
    action = "list"
)]
/// 查询发布修订列表（修订号降序）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 发布 ID
///
/// # 返回
/// 返回修订视图列表。
pub async fn product_publication_revision_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Vec<ProductPublicationRevisionView>> {
    let view = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .revision_list(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "查询发布修订媒体列表",
    resource = "product_publication_revision",
    action = "list"
)]
/// 查询发布修订的受控媒体列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `revision_id` - 发布修订 ID
///
/// # 返回
/// 返回媒体视图列表。
pub async fn product_publication_revision_media_list(
    State(state): State<AppState>,
    Path(revision_id): Path<String>,
) -> Result<Vec<ProductPublicationRevisionMediaView>> {
    let view = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .revision_media_list(&revision_id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "投递发布修订到目标商城",
    resource = "product_publication_delivery",
    action = "submit"
)]
/// 投递发布修订到目标商城（外部调用在事务之外，结果经 `inbox_message` +
/// `integration_error_task` 承接；商城确认后发布推进为商城生效）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 发布 ID
/// * `revision_no` - 修订序号
/// * `req` - 投递请求（含幂等键）
///
/// # 返回
/// 返回投递结果视图。
pub async fn product_publication_delivery_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, revision_no)): Path<(String, u32)>,
    Json(req): Json<DeliverPublicationRevisionRequest>,
) -> Result<PublicationDeliveryResultView> {
    let view = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .deliver_revision(&id, revision_no, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "查询发布投递列表",
    resource = "product_publication_delivery",
    action = "list"
)]
/// 查询发布投递记录列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`target_mall_id` 等扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn product_publication_delivery_list(
    State(state): State<AppState>,
    Query(params): Query<ProductPublicationDeliveryListParams>,
) -> Result<PageView<ProductPublicationDeliveryView>> {
    let page = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .delivery_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "执行发布投递对象动作",
    resource = "product_publication_delivery",
    action = "operate"
)]
/// 对固定发布投递执行查询原结果、受控重试或升级 W29。
pub async fn product_publication_delivery_action(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(delivery_id): Path<String>,
    Json(command): Json<PublicationDeliveryCommand>,
) -> Result<PublicationDeliveryActionResultView> {
    let result = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .apply_publication_delivery_command(&delivery_id, command, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "处理待发送发布投递",
    resource = "product_publication_delivery",
    action = "process_pending"
)]
/// 以有界批次处理待发送与已到期重试投递。
pub async fn product_publication_delivery_process_pending(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<ProcessPublicationDeliveriesRequest>,
) -> Result<ProcessPublicationDeliveriesResult> {
    let result = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .process_pending_publication_deliveries(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "商品发布",
    group_desc = "二期商品发布与投递（W22）",
    desc = "查询系统安全暂停结果",
    resource = "product_publication",
    action = "detail"
)]
/// 按原幂等键查询已落库的系统安全暂停结果。
///
/// 此处只提供只读恢复查询；系统安全暂停触发不暴露给浏览器，必须来自可信
/// 目录/供给服务并加入来源事实的同一事务。
pub async fn product_publication_safety_pause_detail(
    State(state): State<AppState>,
    Path(idempotency_key): Path<String>,
) -> Result<SystemSafetyPauseOperationView> {
    let view = PublicationService::new(state.db(), Arc::new(UnavailableMallConnector))
        .safety_pause_operation(&idempotency_key)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
