//! 域 D18 `receivable` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::receivable` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    receivable::{
        CardFundsReviewDetailParams, CompleteCardFundsReviewCommand, CompleteCardFundsReviewResult,
        CreateCustomerReceiptRequest, CreateInvoiceRequest, CreateReceivableAccountRequest,
        CustomerReceiptListParams, CustomerReceiptView, InvoiceListParams, InvoiceView,
        IssueRedInvoiceRequest, PageView, PostCustomerReceiptRequest, PostInvoiceRequest,
        ReceivableAccountListParams, ReceivableAccountView, ReceivableService,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "查询应收往来子账列表",
    resource = "receivable_account",
    action = "list"
)]
/// 查询应收往来子账列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn receivable_account_list(
    State(state): State<AppState>,
    Query(params): Query<ReceivableAccountListParams>,
) -> Result<PageView<ReceivableAccountView>> {
    let page = ReceivableService::new(state.db())
        .receivable_account_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "查询应收往来子账详情",
    resource = "receivable_account",
    action = "detail"
)]
/// 查询应收往来子账详情（子账 + 分录 + 抵销 + 复核链）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的当前操作人
/// * `id` - 应收往来子账 ID
/// * `params` - W13 正式任务入口参数
///
/// # 返回
/// 返回完整台账视图。
pub async fn receivable_account_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<CardFundsReviewDetailParams>,
) -> Result<ReceivableAccountView> {
    let view = ReceivableService::new(state.db())
        .receivable_account_detail_with_actions(&id, &params, &actor, state.rbac())
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "建立应收往来子账",
    resource = "receivable_account",
    action = "create"
)]
/// 建立应收往来子账与原始应收分录（跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建子账的响应视图。
pub async fn receivable_account_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateReceivableAccountRequest>,
) -> Result<ReceivableAccountView> {
    let view = ReceivableService::new(state.db())
        .create_receivable_account(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "完成卡券票款正式复核",
    resource = "receivable_funds_review",
    action = "complete"
)]
/// 以 W13 强类型命令完成卡券票款正式复核。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `command` - 含任务/对象/账户/链/票款事实版本的完整决定
///
/// # 返回
/// 返回可严格重放的正式结果。
pub async fn receivable_funds_review_complete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<CompleteCardFundsReviewCommand>,
) -> Result<CompleteCardFundsReviewResult> {
    let result = ReceivableService::new(state.db())
        .complete_card_funds_review(command, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "查询客户回款单列表",
    resource = "customer_receipt",
    action = "list"
)]
/// 查询客户回款单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn customer_receipt_list(
    State(state): State<AppState>,
    Query(params): Query<CustomerReceiptListParams>,
) -> Result<PageView<CustomerReceiptView>> {
    let page = ReceivableService::new(state.db())
        .customer_receipt_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "查询客户回款单详情",
    resource = "customer_receipt",
    action = "detail"
)]
/// 查询客户回款单详情（含核销分配行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 回款单 ID
///
/// # 返回
/// 返回回款单视图。
pub async fn customer_receipt_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableService::new(state.db())
        .customer_receipt_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "登记客户回款草稿",
    resource = "customer_receipt",
    action = "create"
)]
/// 登记客户回款草稿（回款单号唯一构成幂等去重）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建回款单视图。
pub async fn customer_receipt_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableService::new(state.db())
        .create_customer_receipt(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "客户回款过账并核销",
    resource = "customer_receipt",
    action = "post"
)]
/// 客户回款过账并核销（§8.3-1 事务不变量，资金入口幂等去重）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 回款单 ID
/// * `req` - 过账请求（核销分配行）
///
/// # 返回
/// 返回过账后回款单视图。
pub async fn customer_receipt_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableService::new(state.db())
        .post_customer_receipt(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "查询发票列表",
    resource = "invoice",
    action = "list"
)]
/// 查询发票列表（销项/进项共用，`invoice_direction` 筛选）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn invoice_list(
    State(state): State<AppState>,
    Query(params): Query<InvoiceListParams>,
) -> Result<PageView<InvoiceView>> {
    let page = ReceivableService::new(state.db()).invoice_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "查询发票详情",
    resource = "invoice",
    action = "detail"
)]
/// 查询发票详情（含分配行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 发票 ID
///
/// # 返回
/// 返回发票视图。
pub async fn invoice_detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<InvoiceView> {
    let view = ReceivableService::new(state.db()).invoice_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "登记发票草稿",
    resource = "invoice",
    action = "create"
)]
/// 登记发票草稿（单集合写入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建发票视图。
pub async fn invoice_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateInvoiceRequest>,
) -> Result<InvoiceView> {
    let view = ReceivableService::new(state.db())
        .create_invoice(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "发票登记过账并分配",
    resource = "invoice",
    action = "post"
)]
/// 发票登记过账并分配（§8.3-2 事务不变量，资金入口幂等去重）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 发票 ID
/// * `req` - 过账请求（分配行）
///
/// # 返回
/// 返回登记后发票视图。
pub async fn invoice_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostInvoiceRequest>,
) -> Result<InvoiceView> {
    let view = ReceivableService::new(state.db())
        .post_invoice(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "开具红票并红冲",
    resource = "invoice",
    action = "reverse"
)]
/// 开具红票并红冲（§8.3-3 事务不变量：保留原事实，累计红冲不超原分配）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 原蓝票 ID
/// * `req` - 红票请求（含红冲分配行）
///
/// # 返回
/// 返回新建红票视图。
pub async fn invoice_red_issue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<IssueRedInvoiceRequest>,
) -> Result<InvoiceView> {
    let view = ReceivableService::new(state.db())
        .issue_red_invoice(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
