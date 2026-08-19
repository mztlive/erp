//! 域 D16 `fulfillment` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::fulfillment` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 履约对象快照查询指纹密钥取 `app.secret` 字节（Service 构造参数）。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    fulfillment::{
        AcceptanceEligibilityView, CreateCustomerAcceptanceRequest, CreateDeliveryRequest,
        CreateElectronicDeliveryRequest, CreatePurchaseReceiptRequest, CreateServiceFulfillmentRequest,
        CustomerAcceptanceDetailView, CustomerAcceptanceListParams, CustomerAcceptanceView,
        DeliveryDetailView, DeliveryListParams, DeliveryView, ElectronicDeliveryListParams,
        ElectronicDeliveryView, FulfillmentService, PageView, PostCustomerAcceptanceRequest,
        PurchaseReceiptDetailView, PurchaseReceiptListParams, PurchaseReceiptView,
        ReverseCustomerAcceptanceRequest, ServiceFulfillmentListParams, ServiceFulfillmentView,
        UpdateDeliveryRequest, UpdatePurchaseReceiptRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

/// 构造履约服务实例（指纹密钥 = `app.secret` 字节）。
///
/// # 参数
/// * `state` - 应用状态
///
/// # 返回
/// 返回履约服务实例。
fn service(state: &AppState) -> FulfillmentService {
    FulfillmentService::new(state.db(), state.config_snapshot().app.secret.as_bytes().to_vec())
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询采购入库单列表",
    resource = "purchase_receipt",
    action = "list"
)]
/// 查询采购入库单列表（W09 入库视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`purchase_order_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn purchase_receipt_list(
    State(state): State<AppState>,
    Query(params): Query<PurchaseReceiptListParams>,
) -> Result<PageView<PurchaseReceiptView>> {
    let page = service(&state).purchase_receipt_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询采购入库单详情",
    resource = "purchase_receipt",
    action = "detail"
)]
/// 查询采购入库单详情（表头 + 行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 入库单主键
///
/// # 返回
/// 返回入库单详情视图。
pub async fn purchase_receipt_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<PurchaseReceiptDetailView> {
    let view = service(&state).purchase_receipt_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "创建采购入库单",
    resource = "purchase_receipt",
    action = "create"
)]
/// 创建采购入库单（草稿）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（表头 + 行）
///
/// # 返回
/// 返回新建入库单的响应视图。
pub async fn purchase_receipt_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePurchaseReceiptRequest>,
) -> Result<PurchaseReceiptView> {
    let view = service(&state).create_purchase_receipt(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "更新采购入库单",
    resource = "purchase_receipt",
    action = "update"
)]
/// 更新采购入库单（仅草稿；乐观锁冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 入库单主键
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后入库单的响应视图。
pub async fn purchase_receipt_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePurchaseReceiptRequest>,
) -> Result<PurchaseReceiptView> {
    let view = service(&state).update_purchase_receipt(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "过账采购入库",
    resource = "purchase_receipt",
    action = "post"
)]
/// 过账采购入库（入库行 + 库存流水 + 余额 + 销售预占同事务，§8.2 第 1 条）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 入库单主键
///
/// # 返回
/// 返回过账后的入库单视图。
pub async fn purchase_receipt_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchaseReceiptView> {
    let view = service(&state).post_purchase_receipt(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询发货单列表",
    resource = "delivery",
    action = "list"
)]
/// 查询发货单列表（W09 发货视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sales_order_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn delivery_list(
    State(state): State<AppState>,
    Query(params): Query<DeliveryListParams>,
) -> Result<PageView<DeliveryView>> {
    let page = service(&state).delivery_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询发货单详情",
    resource = "delivery",
    action = "detail"
)]
/// 查询发货单详情（表头 + 行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 发货单主键
///
/// # 返回
/// 返回发货单详情视图。
pub async fn delivery_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<DeliveryDetailView> {
    let view = service(&state).delivery_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "创建发货单",
    resource = "delivery",
    action = "create"
)]
/// 创建发货单（草稿；仓发/直发表头与行归属由实体校验）。
///
/// 发货为 `NO_APPROVAL`：HTTP 创建路径不启动审批、不接受定义 ID。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（表头 + 行）
///
/// # 返回
/// 返回新建发货单的响应视图。
pub async fn delivery_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDeliveryRequest>,
) -> Result<DeliveryView> {
    let view = service(&state).create_delivery(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "更新发货单",
    resource = "delivery",
    action = "update"
)]
/// 更新发货单（仅草稿；乐观锁冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 发货单主键
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后发货单的响应视图。
pub async fn delivery_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDeliveryRequest>,
) -> Result<DeliveryView> {
    let view = service(&state).update_delivery(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "过账发货",
    resource = "delivery",
    action = "post"
)]
/// 过账发货（仓发：预占消耗 + 出库流水 + 余额同事务，§8.2 第 2 条；直发：只做门槛校验）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 发货单主键
///
/// # 返回
/// 返回过账后的发货单视图。
pub async fn delivery_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<DeliveryView> {
    let view = service(&state).post_delivery(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询电子交付记录列表",
    resource = "electronic_delivery",
    action = "list"
)]
/// 查询电子交付记录列表（W09 电子交付视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sales_order_line_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn electronic_delivery_list(
    State(state): State<AppState>,
    Query(params): Query<ElectronicDeliveryListParams>,
) -> Result<PageView<ElectronicDeliveryView>> {
    let page = service(&state).electronic_delivery_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "创建电子交付记录",
    resource = "electronic_delivery",
    action = "create"
)]
/// 创建电子交付记录（草稿；交付对象快照由边界传入，服务端计算查询指纹）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建记录的响应视图。
pub async fn electronic_delivery_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateElectronicDeliveryRequest>,
) -> Result<ElectronicDeliveryView> {
    let view = service(&state).create_electronic_delivery(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "确认电子交付",
    resource = "electronic_delivery",
    action = "confirm"
)]
/// 确认电子交付（草稿 → 已确认；门槛与分配有效性校验同事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 记录主键
///
/// # 返回
/// 返回确认后的记录视图。
pub async fn electronic_delivery_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<ElectronicDeliveryView> {
    let view = service(&state).confirm_electronic_delivery(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询服务履约记录列表",
    resource = "service_fulfillment",
    action = "list"
)]
/// 查询线下服务履约记录列表（W09 服务视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sales_order_line_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn service_fulfillment_list(
    State(state): State<AppState>,
    Query(params): Query<ServiceFulfillmentListParams>,
) -> Result<PageView<ServiceFulfillmentView>> {
    let page = service(&state).service_fulfillment_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "创建服务履约记录",
    resource = "service_fulfillment",
    action = "create"
)]
/// 创建线下服务履约记录（草稿；服务地点与交付对象快照由边界传入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建记录的响应视图。
pub async fn service_fulfillment_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateServiceFulfillmentRequest>,
) -> Result<ServiceFulfillmentView> {
    let view = service(&state).create_service_fulfillment(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "确认服务履约",
    resource = "service_fulfillment",
    action = "confirm"
)]
/// 确认服务履约（草稿 → 已确认；门槛与分配有效性校验同事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 记录主键
///
/// # 返回
/// 返回确认后的记录视图。
pub async fn service_fulfillment_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<ServiceFulfillmentView> {
    let view = service(&state).confirm_service_fulfillment(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询客户验收单列表",
    resource = "customer_acceptance",
    action = "list"
)]
/// 查询客户验收单列表（W06 验收历史视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sales_order_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn customer_acceptance_list(
    State(state): State<AppState>,
    Query(params): Query<CustomerAcceptanceListParams>,
) -> Result<PageView<CustomerAcceptanceView>> {
    let page = service(&state).customer_acceptance_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询客户验收单详情",
    resource = "customer_acceptance",
    action = "detail"
)]
/// 查询客户验收单详情（表头 + 行 + 分配）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 验收单主键
///
/// # 返回
/// 返回验收单详情视图。
pub async fn customer_acceptance_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<CustomerAcceptanceDetailView> {
    let view = service(&state).customer_acceptance_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "创建客户验收单",
    resource = "customer_acceptance",
    action = "create"
)]
/// 创建客户验收单（草稿；分配在过账时校验并写入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（表头 + 行）
///
/// # 返回
/// 返回新建验收单的响应视图。
pub async fn customer_acceptance_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerAcceptanceRequest>,
) -> Result<CustomerAcceptanceView> {
    let view = service(&state).create_customer_acceptance(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "过账客户验收",
    resource = "customer_acceptance",
    action = "post"
)]
/// 过账客户验收（验收行锁定 + 履约分配守恒 + 净验收上限校验同事务，§8.2 第 5 条）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 验收单主键
/// * `req` - 过账请求（逐行分配）
///
/// # 返回
/// 返回过账后的验收单视图。
pub async fn customer_acceptance_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostCustomerAcceptanceRequest>,
) -> Result<CustomerAcceptanceView> {
    let view = service(&state).post_customer_acceptance(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "冲正客户验收",
    resource = "customer_acceptance",
    action = "reverse"
)]
/// 冲正客户验收（误录时新增反向验收与反向分配，不覆盖原事实）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 待冲正验收单主键
/// * `req` - 冲正请求（期望版本 + 原因）
///
/// # 返回
/// 返回新建反向验收单的视图。
pub async fn customer_acceptance_reverse(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReverseCustomerAcceptanceRequest>,
) -> Result<CustomerAcceptanceView> {
    let view = service(&state)
        .reverse_customer_acceptance(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询客户验收工作台",
    resource = "customer_acceptance",
    action = "list"
)]
/// 查询客户验收工作台（W06：销售行 + 可验收事实 + 验收历史）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 销售单（`sales_order_id` 必填）
///
/// # 返回
/// 返回验收工作台视图。
pub async fn customer_acceptance_eligible(
    State(state): State<AppState>,
    Query(params): Query<CustomerAcceptanceListParams>,
) -> Result<AcceptanceEligibilityView> {
    let sales_order_id = params
        .sales_order_id
        .clone()
        .ok_or_else(|| services::Error::ValidationError("sales_order_id 不能为空".to_string()))?;
    let view = service(&state)
        .acceptance_eligibility(sales_order_id.as_ref())
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[cfg(test)]
mod tests {
    use services::fulfillment::{CreateDeliveryRequest, CreatePurchaseReceiptRequest};

    /// 采购收货 HTTP 只暴露创建/过账，不得提交审批或选择定义。
    #[test]
    fn purchase_receipt_http_proves_no_approval() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("create_purchase_receipt"));
        assert!(production.contains("purchase_receipt_create"));
        assert!(production.contains("purchase_receipt_detail"));
        assert!(production.contains("purchase_receipt_post"));
        assert!(!production.contains("submit_purchase_receipt"));
        assert!(!production.contains("purchase_receipt_submit"));
        assert!(!production.contains("cancel_purchase_receipt"));
        assert!(!production.contains("purchase_receipt_cancel"));
        assert!(!production.contains("start_purchase_receipt"));
        assert!(!production.contains("PurchaseReceiptAdapter"));
        let create = production
            .split("pub async fn purchase_receipt_create")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn purchase_receipt_update").next())
            .expect("purchase_receipt_create 生产片段");
        assert!(create.contains("create_purchase_receipt"));
        assert!(!create.contains("submit_"));
        assert!(!create.contains("start_approval"));
        assert!(!create.contains("definition_id"));
        let post = production
            .split("pub async fn purchase_receipt_post")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn delivery_list").next())
            .expect("purchase_receipt_post 生产片段");
        assert!(post.contains("post_purchase_receipt"));
        assert!(!post.contains("start_approval"));
        assert!(!post.contains("WorkItem"));
        assert!(!post.contains("definition_id"));
        assert!(
            serde_json::from_value::<CreatePurchaseReceiptRequest>(serde_json::json!({
                "receipt_no": "PR-1",
                "purchase_order_id": "po-1",
                "warehouse_id": "wh-1",
                "lines": [{
                    "purchase_order_revision_line_id": "porl-1",
                    "received_quantity": "10",
                    "qualified_quantity": "10",
                    "rejected_quantity": "0"
                }],
                "definition_id": "forged",
                "assignee": "forged"
            }))
            .is_err()
        );
    }

    /// 发货 HTTP 只暴露创建/过账，不得提交审批或选择定义。
    #[test]
    fn delivery_http_proves_no_approval() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("create_delivery"));
        assert!(production.contains("delivery_create"));
        assert!(production.contains("delivery_detail"));
        assert!(production.contains("delivery_post"));
        assert!(!production.contains("submit_delivery"));
        assert!(!production.contains("delivery_submit"));
        assert!(!production.contains("cancel_delivery"));
        assert!(!production.contains("delivery_cancel"));
        assert!(!production.contains("start_delivery"));
        assert!(!production.contains("DeliveryAdapter"));
        let create = production
            .split("pub async fn delivery_create")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn delivery_update").next())
            .expect("delivery_create 生产片段");
        assert!(create.contains("create_delivery"));
        assert!(!create.contains("submit_"));
        assert!(!create.contains("start_approval"));
        assert!(!create.contains("definition_id"));
        let post = production
            .split("pub async fn delivery_post")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn electronic_delivery_list").next())
            .expect("delivery_post 生产片段");
        assert!(post.contains("post_delivery"));
        assert!(!post.contains("start_approval"));
        assert!(!post.contains("WorkItem"));
        assert!(!post.contains("definition_id"));
        assert!(
            serde_json::from_value::<CreateDeliveryRequest>(serde_json::json!({
                "delivery_no": "DV-1",
                "delivery_type": "WAREHOUSE_SHIP",
                "sales_order_id": "so-1",
                "warehouse_id": "wh-1",
                "lines": [{
                    "sales_order_line_id": "so-line-1",
                    "quantity": "2",
                    "stock_reservation_id": "rsv-1"
                }]
            }))
            .is_ok()
        );
        assert!(
            serde_json::from_value::<CreateDeliveryRequest>(serde_json::json!({
                "delivery_no": "DV-1",
                "delivery_type": "WAREHOUSE_SHIP",
                "sales_order_id": "so-1",
                "warehouse_id": "wh-1",
                "lines": [{
                    "sales_order_line_id": "so-line-1",
                    "quantity": "2",
                    "stock_reservation_id": "rsv-1"
                }],
                "definition_id": "forged",
                "assignee": "forged"
            }))
            .is_err()
        );
    }
}
