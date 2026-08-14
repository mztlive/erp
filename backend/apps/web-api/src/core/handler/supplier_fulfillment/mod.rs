//! 域 D32 `supplier_fulfillment` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::supplier_fulfillment` 的 DTO，禁止重复定义同构类型、
//! 禁止直连数据库。未注入受控生产 Connector 时固定失败关闭，不得伪造外部成功。

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    supplier_fulfillment::{
        PageView, PlaceFulfillmentOrderRequest, RecordRefundResultRequest, RecordSupplierRejectRequest,
        SubmitActionResultView, SubmitAfterSalesActionRequest, SupplierFulfillmentOrderDetailParams,
        SupplierFulfillmentOrderDetailView, SupplierFulfillmentOrderListParams, SupplierFulfillmentOrderView,
        SupplierFulfillmentService, SupplierOrderInvestigationResultView,
        SupplierOrderObjectInvestigationCommand, SupplierOrderStatusHistoryView,
        SupplierOrderTaskCompletionCommand, SupplierOrderTaskCompletionResultView,
        SupplierOrderTaskInvestigationCommand, SupplierRefundFactView, UnavailableSupplierGateway,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "查询供应商履约订单列表",
    resource = "supplier_fulfillment_order",
    action = "list"
)]
/// 查询供应商履约订单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`supplier_id`/三条状态/`external_order_no` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn supplier_fulfillment_order_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierFulfillmentOrderListParams>,
) -> Result<PageView<SupplierFulfillmentOrderView>> {
    let page = SupplierFulfillmentService::new(state.db(), default_gateway())
        .supplier_fulfillment_order_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "查询供应商履约订单详情",
    resource = "supplier_fulfillment_order",
    action = "detail"
)]
/// 查询供应商履约订单详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的当前操作人
/// * `id` - 供应商子订单 ID
/// * `params` - W26 正式任务入口参数
///
/// # 返回
/// 返回订单详情视图（订单 + 明细 + 状态历史 + 动作 + 退款事实）。
pub async fn supplier_fulfillment_order_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<SupplierFulfillmentOrderDetailParams>,
) -> Result<SupplierFulfillmentOrderDetailView> {
    let view = SupplierFulfillmentService::new(state.db(), default_gateway())
        .supplier_fulfillment_order_detail(&id, &params, &actor, state.rbac())
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "从订单入口调查供应商原动作结果",
    resource = "supplier_fulfillment_order",
    action = "investigate"
)]
/// 从普通订单入口查询原结果或执行已证明安全的重放。
pub async fn supplier_fulfillment_order_investigation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<SupplierOrderObjectInvestigationCommand>,
) -> Result<SupplierOrderInvestigationResultView> {
    let view = SupplierFulfillmentService::new(state.db(), default_gateway())
        .investigate_order(command, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "从正式任务调查供应商原动作结果",
    resource = "supplier_fulfillment_order",
    action = "investigate"
)]
/// 从 W26 正式任务入口查询原结果或执行已证明安全的重放。
pub async fn supplier_fulfillment_order_task_investigation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<SupplierOrderTaskInvestigationCommand>,
) -> Result<SupplierOrderInvestigationResultView> {
    let view = SupplierFulfillmentService::new(state.db(), default_gateway())
        .investigate_order_task(command, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "根据已验证结果完成供应商履约任务",
    resource = "supplier_fulfillment_order",
    action = "complete"
)]
/// 以服务端可验证终态证据完成 W26 正式任务。
pub async fn supplier_fulfillment_order_task_completion(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<SupplierOrderTaskCompletionCommand>,
) -> Result<SupplierOrderTaskCompletionResultView> {
    let view = SupplierFulfillmentService::new(state.db(), default_gateway())
        .complete_order_task(command, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "向供应商提交下单",
    resource = "supplier_fulfillment_order",
    action = "submit"
)]
/// 向供应商提交下单（幂等键：`fulfillment_order_no`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 下单请求
///
/// # 返回
/// 返回下单后订单的响应视图。
pub async fn supplier_fulfillment_order_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<PlaceFulfillmentOrderRequest>,
) -> Result<SupplierFulfillmentOrderView> {
    let view = SupplierFulfillmentService::new(state.db(), default_gateway())
        .submit_place(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "向供应商提交取消",
    resource = "supplier_fulfillment_order",
    action = "cancel"
)]
/// 向供应商提交取消（幂等键：「订单号 + CANCEL + 售后申请 ID」）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商子订单 ID
/// * `req` - 取消动作提交请求
///
/// # 返回
/// 返回动作与动作后订单视图。
pub async fn supplier_fulfillment_order_cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitAfterSalesActionRequest>,
) -> Result<SubmitActionResultView> {
    let view = SupplierFulfillmentService::new(state.db(), default_gateway())
        .submit_cancel(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "向供应商提交退款",
    resource = "supplier_fulfillment_order",
    action = "refund"
)]
/// 向供应商提交退款（幂等键：「订单号 + REFUND + 售后申请 ID」）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商子订单 ID
/// * `req` - 退款动作提交请求
///
/// # 返回
/// 返回动作与动作后订单视图。
pub async fn supplier_fulfillment_order_refund(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitAfterSalesActionRequest>,
) -> Result<SubmitActionResultView> {
    let view = SupplierFulfillmentService::new(state.db(), default_gateway())
        .submit_refund(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "登记供应商拒单结果",
    resource = "supplier_fulfillment_order",
    action = "reject"
)]
/// 登记供应商拒单结果（回调幂等键 `(connection_id, external_event_id)`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商子订单 ID
/// * `req` - 拒单结果请求
///
/// # 返回
/// 返回新增的状态历史视图。
pub async fn supplier_fulfillment_order_reject(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RecordSupplierRejectRequest>,
) -> Result<SupplierOrderStatusHistoryView> {
    let view = SupplierFulfillmentService::new(state.db(), default_gateway())
        .record_reject(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "登记供应商退款成功结果",
    resource = "supplier_refund_fact",
    action = "post"
)]
/// 登记供应商退款成功结果（幂等键 `(connection_id, external_refund_no,
/// external_refund_version)`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商子订单 ID
/// * `req` - 退款成功结果请求
///
/// # 返回
/// 返回退款事实视图（含分配行）。
pub async fn supplier_refund_fact_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RecordRefundResultRequest>,
) -> Result<SupplierRefundFactView> {
    let view = SupplierFulfillmentService::new(state.db(), default_gateway())
        .record_refund_result(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 构造默认供应商网关（模拟网关：按连接地址配置分类，不发起真实网络请求）。
///
/// # 返回
/// 返回线程安全的网关实例。
fn default_gateway() -> Arc<dyn services::supplier_fulfillment::SupplierGateway> {
    Arc::new(UnavailableSupplierGateway)
}
