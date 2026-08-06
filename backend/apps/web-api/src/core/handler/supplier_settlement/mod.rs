//! 域 D33 `supplier_settlement` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::supplier_settlement` 的 DTO，禁止重复定义同构类型、
//! 禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    supplier_settlement::{
        ConfirmSettlementRequest, CreateSettlementStatementRequest, ResolveDifferenceRequest,
        SettlementPageView, SubmitSettlementReviewRequest, SupplierSettlementDifferenceListParams,
        SupplierSettlementDifferenceView, SupplierSettlementItemListParams, SupplierSettlementItemView,
        SupplierSettlementService, SupplierSettlementStatementDetailView,
        SupplierSettlementStatementListParams, SupplierSettlementStatementView, VoidSettlementRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "查询供应商结算单列表",
    resource = "supplier_settlement_statement",
    action = "list"
)]
/// 查询供应商结算单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`statement_no`/`supplier_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn supplier_settlement_statement_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierSettlementStatementListParams>,
) -> Result<SettlementPageView<SupplierSettlementStatementView>> {
    let page = SupplierSettlementService::new(state.db())
        .supplier_settlement_statement_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "查询供应商结算单详情",
    resource = "supplier_settlement_statement",
    action = "detail"
)]
/// 查询供应商结算单详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 结算单 ID
///
/// # 返回
/// 返回结算单详情视图（结算单 + 明细 + 差异）。
pub async fn supplier_settlement_statement_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SupplierSettlementStatementDetailView> {
    let view = SupplierSettlementService::new(state.db())
        .supplier_settlement_statement_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "创建供应商结算草稿",
    resource = "supplier_settlement_statement",
    action = "create"
)]
/// 创建供应商结算草稿（幂等键：`statement_no`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人（经办人）
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建结算单的响应视图。
pub async fn supplier_settlement_statement_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSettlementStatementRequest>,
) -> Result<SupplierSettlementStatementView> {
    let view = SupplierSettlementService::new(state.db())
        .create_statement(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "提交结算复核",
    resource = "supplier_settlement_statement",
    action = "submit"
)]
/// 提交结算复核（乐观锁；存在未解决差异时拒绝）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 结算单 ID
/// * `req` - 提交请求（含期望版本）
///
/// # 返回
/// 返回提交后结算单的响应视图。
pub async fn supplier_settlement_statement_submit_review(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSettlementReviewRequest>,
) -> Result<SupplierSettlementStatementView> {
    let view = SupplierSettlementService::new(state.db())
        .submit_review(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "确认结算并形成应付",
    resource = "supplier_settlement_statement",
    action = "confirm"
)]
/// 确认结算（乐观锁；同事务形成结算单应付，§8.4 第 6 条；重复确认返回原结果）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 结算单 ID
/// * `req` - 确认请求（含期望版本与复核人）
///
/// # 返回
/// 返回确认后结算单的响应视图。
pub async fn supplier_settlement_statement_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ConfirmSettlementRequest>,
) -> Result<SupplierSettlementStatementView> {
    let view = SupplierSettlementService::new(state.db())
        .confirm(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "作废结算单",
    resource = "supplier_settlement_statement",
    action = "void"
)]
/// 作废结算单（乐观锁；已作废为终态）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 结算单 ID
/// * `req` - 作废请求（含期望版本与原因）
///
/// # 返回
/// 返回作废后结算单的响应视图。
pub async fn supplier_settlement_statement_void(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidSettlementRequest>,
) -> Result<SupplierSettlementStatementView> {
    let view = SupplierSettlementService::new(state.db())
        .void_statement(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "查询供应商结算明细列表",
    resource = "supplier_settlement_item",
    action = "list"
)]
/// 查询供应商结算明细列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`statement_id` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_settlement_item_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierSettlementItemListParams>,
) -> Result<SettlementPageView<SupplierSettlementItemView>> {
    let page = SupplierSettlementService::new(state.db())
        .supplier_settlement_item_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "查询供应商结算差异列表",
    resource = "supplier_settlement_difference",
    action = "list"
)]
/// 查询供应商结算差异列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`statement_item_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn supplier_settlement_difference_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierSettlementDifferenceListParams>,
) -> Result<SettlementPageView<SupplierSettlementDifferenceView>> {
    let page = SupplierSettlementService::new(state.db())
        .supplier_settlement_difference_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "登记结算差异处理结论",
    resource = "supplier_settlement_difference",
    action = "update"
)]
/// 登记结算差异处理结论（乐观锁；处理结果三元组成组校验）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 结算差异 ID
/// * `req` - 处理请求（含期望版本与结论状态）
///
/// # 返回
/// 返回处理后差异的响应视图。
pub async fn supplier_settlement_difference_resolve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ResolveDifferenceRequest>,
) -> Result<SupplierSettlementDifferenceView> {
    let view = SupplierSettlementService::new(state.db())
        .resolve_difference(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
