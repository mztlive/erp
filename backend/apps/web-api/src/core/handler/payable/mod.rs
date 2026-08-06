//! 域 D19 `payable` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::payable` 的 DTO。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    payable::{
        CreatePayableAccountRequest, CreateSupplierPaymentRequest, PageView, PayableAccountListParams,
        PayableAccountView, PayableService, PostSupplierPaymentRequest, PurchaseInvoiceAllocationListParams,
        PurchaseInvoiceAllocationView, PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest,
        SupplierPaymentListParams, SupplierPaymentView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "查询应付往来子账列表",
    resource = "payable_account",
    action = "list"
)]
/// 查询应付往来子账列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn payable_account_list(
    State(state): State<AppState>,
    Query(params): Query<PayableAccountListParams>,
) -> Result<PageView<PayableAccountView>> {
    let page = PayableService::new(state.db())
        .payable_account_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "查询应付往来子账详情",
    resource = "payable_account",
    action = "detail"
)]
/// 查询应付往来子账详情（子账 + 分录）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 应付往来子账 ID
///
/// # 返回
/// 返回完整应付台账视图。
pub async fn payable_account_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<PayableAccountView> {
    let view = PayableService::new(state.db())
        .payable_account_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "建立应付往来子账",
    resource = "payable_account",
    action = "create"
)]
/// 建立应付往来子账与原始应付分录（跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建子账的响应视图。
pub async fn payable_account_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePayableAccountRequest>,
) -> Result<PayableAccountView> {
    let view = PayableService::new(state.db())
        .create_payable_account(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "查询供应商付款单列表",
    resource = "supplier_payment",
    action = "list"
)]
/// 查询供应商付款单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_payment_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierPaymentListParams>,
) -> Result<PageView<SupplierPaymentView>> {
    let page = PayableService::new(state.db())
        .supplier_payment_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "查询供应商付款单详情",
    resource = "supplier_payment",
    action = "detail"
)]
/// 查询供应商付款单详情（含核销分配行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 付款单 ID
///
/// # 返回
/// 返回付款单视图。
pub async fn supplier_payment_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SupplierPaymentView> {
    let view = PayableService::new(state.db())
        .supplier_payment_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "登记供应商付款草稿",
    resource = "supplier_payment",
    action = "create"
)]
/// 登记供应商付款草稿（付款单号唯一构成幂等去重）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建付款单视图。
pub async fn supplier_payment_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierPaymentRequest>,
) -> Result<SupplierPaymentView> {
    let view = PayableService::new(state.db())
        .create_supplier_payment(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "供应商付款过账并核销",
    resource = "supplier_payment",
    action = "post"
)]
/// 供应商付款过账并核销（§8.3-1 事务不变量，资金入口幂等去重）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 付款单 ID
/// * `req` - 过账请求（核销分配行）
///
/// # 返回
/// 返回过账后付款单视图。
pub async fn supplier_payment_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostSupplierPaymentRequest>,
) -> Result<SupplierPaymentView> {
    let view = PayableService::new(state.db())
        .post_supplier_payment(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "进项发票登记过账并分配",
    resource = "purchase_invoice_allocation",
    action = "post"
)]
/// 进项发票登记过账并分配（§8.3-2 事务不变量，资金入口幂等去重）。
///
/// 发票实体经 D18 `invoices()` 仓储写入，D19 只写进项发票分配。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 进项发票登记请求
///
/// # 返回
/// 返回登记后发票与分配行视图。
pub async fn purchase_invoice_allocation_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterPurchaseInvoiceRequest>,
) -> Result<PurchaseInvoiceRegisteredView> {
    let view = PayableService::new(state.db())
        .register_purchase_invoice(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "查询进项发票分配列表",
    resource = "purchase_invoice_allocation",
    action = "list"
)]
/// 查询进项发票分配列表（按应付子账筛选）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`payable_account_id` 必填）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn purchase_invoice_allocation_list(
    State(state): State<AppState>,
    Query(params): Query<PurchaseInvoiceAllocationListParams>,
) -> Result<PageView<PurchaseInvoiceAllocationView>> {
    let page = PayableService::new(state.db())
        .purchase_invoice_allocation_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}
