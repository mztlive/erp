//! 域 D21 `returns` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::returns` 的 DTO。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    returns::{
        CreateCustomerRefundRequest, CreatePaymentReversalRequest, CreatePurchaseReturnOrderRequest,
        CreateReceiptReversalRequest, CreateSalesReturnCaseRequest, CreateSupplierRefundRequest,
        CustomerRefundListParams, CustomerRefundView, PageView, PaymentReversalView,
        PostCustomerRefundRequest, PostPaymentReversalRequest, PostReceiptReversalRequest,
        PostSupplierRefundRequest, PurchaseReturnOrderListParams, PurchaseReturnOrderView,
        ReceiptReversalView, ReturnsService, SalesReturnCaseListParams, SalesReturnCaseView,
        SupplierRefundView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询销售退货处理单列表",
    resource = "sales_return_case",
    action = "list"
)]
/// 查询销售退货/拒收处理单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sales_return_case_list(
    State(state): State<AppState>,
    Query(params): Query<SalesReturnCaseListParams>,
) -> Result<PageView<SalesReturnCaseView>> {
    let page = ReturnsService::new(state.db())
        .sales_return_case_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询销售退货处理单详情",
    resource = "sales_return_case",
    action = "detail"
)]
/// 查询销售退货/拒收处理单详情（处理单 + 明细行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 处理单 ID
///
/// # 返回
/// 返回完整处理单视图。
pub async fn sales_return_case_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SalesReturnCaseView> {
    let view = ReturnsService::new(state.db())
        .sales_return_case_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "建立销售退货处理单",
    resource = "sales_return_case",
    action = "create"
)]
/// 建立销售退货/拒收处理单与明细行（跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建处理单视图。
pub async fn sales_return_case_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSalesReturnCaseRequest>,
) -> Result<SalesReturnCaseView> {
    let view = ReturnsService::new(state.db())
        .create_sales_return_case(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询采购退货单列表",
    resource = "purchase_return_order",
    action = "list"
)]
/// 查询采购退货单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn purchase_return_order_list(
    State(state): State<AppState>,
    Query(params): Query<PurchaseReturnOrderListParams>,
) -> Result<PageView<PurchaseReturnOrderView>> {
    let page = ReturnsService::new(state.db())
        .purchase_return_order_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询采购退货单详情",
    resource = "purchase_return_order",
    action = "detail"
)]
/// 查询采购退货单详情（退货单 + 明细行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 退货单 ID
///
/// # 返回
/// 返回完整退货单视图。
pub async fn purchase_return_order_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<PurchaseReturnOrderView> {
    let view = ReturnsService::new(state.db())
        .purchase_return_order_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "建立采购退货单",
    resource = "purchase_return_order",
    action = "create"
)]
/// 建立采购退货单与明细行（跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建退货单视图。
pub async fn purchase_return_order_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePurchaseReturnOrderRequest>,
) -> Result<PurchaseReturnOrderView> {
    let view = ReturnsService::new(state.db())
        .create_purchase_return_order(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询客户退款列表",
    resource = "customer_refund",
    action = "list"
)]
/// 查询客户退款列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn customer_refund_list(
    State(state): State<AppState>,
    Query(params): Query<CustomerRefundListParams>,
) -> Result<PageView<CustomerRefundView>> {
    let page = ReturnsService::new(state.db())
        .customer_refund_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询客户退款详情",
    resource = "customer_refund",
    action = "detail"
)]
/// 查询客户退款详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 退款单 ID
///
/// # 返回
/// 返回退款单视图。
pub async fn customer_refund_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<CustomerRefundView> {
    let view = ReturnsService::new(state.db())
        .customer_refund_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "登记客户退款草稿",
    resource = "customer_refund",
    action = "create"
)]
/// 登记客户退款草稿（退款单号唯一构成幂等去重；经办/复核分离）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建退款单视图。
pub async fn customer_refund_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerRefundRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsService::new(state.db())
        .create_customer_refund(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "客户退款过账",
    resource = "customer_refund",
    action = "post"
)]
/// 客户退款过账（§8.3-3 事务不变量：反向事实 + 反向分录 + 反向核销）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 退款单 ID
/// * `req` - 过账请求（占位）
///
/// # 返回
/// 返回过账后退款单视图。
pub async fn customer_refund_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostCustomerRefundRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsService::new(state.db())
        .post_customer_refund(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "登记供应商退款草稿",
    resource = "supplier_refund",
    action = "create"
)]
/// 登记供应商退款草稿（退款单号唯一构成幂等去重；经办/复核分离）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建退款单视图。
pub async fn supplier_refund_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsService::new(state.db())
        .create_supplier_refund(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "供应商退款过账",
    resource = "supplier_refund",
    action = "post"
)]
/// 供应商退款过账（§8.3-3 事务不变量，应付侧镜像）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 退款单 ID
/// * `req` - 过账请求（占位）
///
/// # 返回
/// 返回过账后退款单视图。
pub async fn supplier_refund_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsService::new(state.db())
        .post_supplier_refund(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "登记回款冲正草稿",
    resource = "receipt_reversal",
    action = "create"
)]
/// 登记回款冲正草稿（冲正单号唯一构成幂等去重；经办/复核分离）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建冲正单视图。
pub async fn receipt_reversal_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateReceiptReversalRequest>,
) -> Result<ReceiptReversalView> {
    let view = ReturnsService::new(state.db())
        .create_receipt_reversal(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "回款冲正过账",
    resource = "receipt_reversal",
    action = "post"
)]
/// 回款冲正过账（§8.3-3 事务不变量：保留原事实，写反向核销）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 冲正单 ID
/// * `req` - 过账请求（占位）
///
/// # 返回
/// 返回过账后冲正单视图。
pub async fn receipt_reversal_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostReceiptReversalRequest>,
) -> Result<ReceiptReversalView> {
    let view = ReturnsService::new(state.db())
        .post_receipt_reversal(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "登记付款冲正草稿",
    resource = "payment_reversal",
    action = "create"
)]
/// 登记付款冲正草稿（冲正单号唯一构成幂等去重；经办/复核分离）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建冲正单视图。
pub async fn payment_reversal_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePaymentReversalRequest>,
) -> Result<PaymentReversalView> {
    let view = ReturnsService::new(state.db())
        .create_payment_reversal(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "付款冲正过账",
    resource = "payment_reversal",
    action = "post"
)]
/// 付款冲正过账（§8.3-3 事务不变量，应付侧镜像）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 冲正单 ID
/// * `req` - 过账请求（占位）
///
/// # 返回
/// 返回过账后冲正单视图。
pub async fn payment_reversal_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostPaymentReversalRequest>,
) -> Result<PaymentReversalView> {
    let view = ReturnsService::new(state.db())
        .post_payment_reversal(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
