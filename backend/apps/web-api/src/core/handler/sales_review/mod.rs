//! 域 D14 `sales_review` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::sales_review` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    sales_review::{
        ApproveProcurementConfirmationRequest, ChangeReviewDecisionRequest, CreateSalesChangeOrderRequest,
        PageView, ProcurementConfirmationDecisionView, ProcurementConfirmationDetailView,
        ProcurementConfirmationListParams, ProcurementConfirmationView, RejectProcurementConfirmationRequest,
        ReviewDecisionRequest, SalesChangeOrderDetailView, SalesChangeOrderListParams, SalesChangeOrderView,
        SalesOrderReviewListParams, SalesOrderReviewView, SalesReviewService,
        SaveProcurementConfirmationLinesRequest, SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "查询销售审批记录列表",
    resource = "sales_order_review",
    action = "list"
)]
/// 查询销售审批记录列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sales_order_review_list(
    State(state): State<AppState>,
    Query(params): Query<SalesOrderReviewListParams>,
) -> Result<PageView<SalesOrderReviewView>> {
    let page = SalesReviewService::new(state.db())
        .sales_order_review_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "通过卡券销售审批",
    resource = "sales_order_review",
    action = "approve"
)]
/// 通过卡券销售审批（销售领导 → 待运营；运营 → 生效事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 审批记录 ID
/// * `req` - 决策请求（意见可空）
///
/// # 返回
/// 返回审批记录视图。
pub async fn sales_order_review_approve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReviewDecisionRequest>,
) -> Result<SalesOrderReviewView> {
    let view = SalesReviewService::new(state.db())
        .approve_sales_order_review(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "驳回卡券销售审批",
    resource = "sales_order_review",
    action = "reject"
)]
/// 驳回卡券销售审批（销售单回草稿，提交标记驳回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 审批记录 ID
/// * `req` - 决策请求（驳回原因必填）
///
/// # 返回
/// 返回审批记录视图。
pub async fn sales_order_review_reject(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReviewDecisionRequest>,
) -> Result<SalesOrderReviewView> {
    let view = SalesReviewService::new(state.db())
        .reject_sales_order_review(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "查询采购确认队列",
    resource = "procurement_confirmation",
    action = "list"
)]
/// 查询采购二次确认队列（W07）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`submission_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn procurement_confirmation_list(
    State(state): State<AppState>,
    Query(params): Query<ProcurementConfirmationListParams>,
) -> Result<PageView<ProcurementConfirmationView>> {
    let page = SalesReviewService::new(state.db())
        .procurement_confirmation_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "查询采购确认详情",
    resource = "procurement_confirmation",
    action = "detail"
)]
/// 查询采购二次确认详情（批次 + 分行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 确认批次 ID
///
/// # 返回
/// 返回详情视图。
pub async fn procurement_confirmation_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ProcurementConfirmationDetailView> {
    let view = SalesReviewService::new(state.db())
        .procurement_confirmation_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "保存采购确认分行",
    resource = "procurement_confirmation",
    action = "update"
)]
/// 保存采购确认分行（W07 草稿编辑，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 确认批次 ID
/// * `req` - 保存请求（含期望版本与分行清单）
///
/// # 返回
/// 返回保存后的详情视图。
pub async fn procurement_confirmation_save_lines(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveProcurementConfirmationLinesRequest>,
) -> Result<ProcurementConfirmationDetailView> {
    let view = SalesReviewService::new(state.db())
        .save_procurement_confirmation_lines(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "采购确认通过",
    resource = "procurement_confirmation",
    action = "approve"
)]
/// 采购确认通过（§8.1.1 单事务：版本 + 销售状态 + 应收 + 待办 + 审计原子生效）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 确认批次 ID
/// * `req` - 通过请求（幂等键）
///
/// # 返回
/// 返回决策结果视图（含生效版本与应收子账）。
pub async fn procurement_confirmation_approve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ApproveProcurementConfirmationRequest>,
) -> Result<ProcurementConfirmationDecisionView> {
    let view = SalesReviewService::new(state.db())
        .approve_procurement_confirmation(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "采购确认驳回",
    resource = "procurement_confirmation",
    action = "reject"
)]
/// 采购确认驳回（销售单回到销售可处理草稿）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 确认批次 ID
/// * `req` - 驳回请求（原因代码必填）
///
/// # 返回
/// 返回决策结果视图。
pub async fn procurement_confirmation_reject(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RejectProcurementConfirmationRequest>,
) -> Result<ProcurementConfirmationDecisionView> {
    let view = SalesReviewService::new(state.db())
        .reject_procurement_confirmation(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "查询销售变更单列表",
    resource = "sales_change_order",
    action = "list"
)]
/// 查询销售变更单列表（W05 变更轨）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sales_order_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sales_change_order_list(
    State(state): State<AppState>,
    Query(params): Query<SalesChangeOrderListParams>,
) -> Result<PageView<SalesChangeOrderView>> {
    let page = SalesReviewService::new(state.db())
        .sales_change_order_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "查询销售变更单详情",
    resource = "sales_change_order",
    action = "detail"
)]
/// 查询销售变更单详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 变更单 ID
///
/// # 返回
/// 返回详情视图。
pub async fn sales_change_order_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .sales_change_order_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "创建销售变更单",
    resource = "sales_change_order",
    action = "create"
)]
/// 创建销售变更单（草稿 + 变更工作副本原子形成）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（含变更类型、原因与目标草稿）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSalesChangeOrderRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .create_sales_change_order(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "发起销售变更影响确认",
    resource = "sales_change_order",
    action = "submit"
)]
/// 发起销售变更影响确认（形成不可变变更提交并进入影响确认/财务复核链）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 提交请求（含期望版本与幂等键）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_submit_impact(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSalesChangeRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .submit_sales_change(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "通过变更履约影响确认",
    resource = "sales_change_order",
    action = "approve"
)]
/// 通过变更履约影响确认（进入财务复核）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 决策请求（意见可空）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_confirm_impact(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ChangeReviewDecisionRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .confirm_impact(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "驳回变更履约影响确认",
    resource = "sales_change_order",
    action = "reject"
)]
/// 驳回变更履约影响确认（变更单回驳回态）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 决策请求（驳回原因必填）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_reject_impact(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ChangeReviewDecisionRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .reject_impact(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "通过变更财务复核",
    resource = "sales_change_order",
    action = "approve"
)]
/// 通过变更财务复核（§8.1.3 变更生效：新版本 + 应收差额 + 当前版本切换）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 决策请求（意见可空）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_confirm_finance(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ChangeReviewDecisionRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .confirm_finance(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "驳回变更财务复核",
    resource = "sales_change_order",
    action = "reject"
)]
/// 驳回变更财务复核（变更单回驳回态）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 决策请求（驳回原因必填）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_reject_finance(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ChangeReviewDecisionRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .reject_finance(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "作废销售变更单",
    resource = "sales_change_order",
    action = "delete"
)]
/// 作废销售变更单（仅草稿态，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 作废请求（含期望版本）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_void(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidSalesChangeOrderRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .void_sales_change(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
