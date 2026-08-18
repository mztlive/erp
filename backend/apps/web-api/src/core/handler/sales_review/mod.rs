//! 域 D14 `sales_review` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::sales_review` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    sales_review::{
        CancelCardSalesApprovalCommand, CancelCardSalesApprovalResult, CancelSalesChangeApprovalRequest,
        CardSalesApprovalActionPort, ChangeReviewDecisionRequest,
        CompleteLowMarginManagerConfirmationCommand, CompleteLowMarginManagerConfirmationResult,
        CompleteProcurementConfirmationCommand, CompleteProcurementConfirmationResult,
        CreateSalesChangeOrderRequest, PageView, ProcurementConfirmationDetailParams,
        ProcurementConfirmationDetailView, ProcurementConfirmationListParams, ProcurementConfirmationView,
        ProcurementRecommendationView, SalesChangeOrderDetailView, SalesChangeOrderListParams,
        SalesChangeOrderView, SalesOrderReviewListParams, SalesOrderReviewView, SalesReviewService,
        SaveProcurementConfirmationLinesRequest, SaveProcurementConfirmationResult,
        SubmitCardSalesApprovalDecisionCommand, SubmitCardSalesApprovalDecisionResult,
        SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
    },
};

#[allow(dead_code)]
#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "完成低毛利上级确认",
    resource = "sales_order_review",
    action = "low_margin_decide"
)]
/// 低毛利上级确认端点已禁用，保留至 P0-D 删除。
pub async fn low_margin_manager_confirmation_decide(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<CompleteLowMarginManagerConfirmationCommand>,
) -> Result<CompleteLowMarginManagerConfirmationResult> {
    let result = SalesReviewService::new(state.db())
        .complete_low_margin_manager_confirmation(command, &actor, state.rbac())
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

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
    desc = "提交卡券销售审批决定",
    resource = "sales_order_review",
    action = "decide"
)]
/// 提交卡券销售当前活动步骤的唯一正式决定。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `command` - W05 强类型决定信封
///
/// # 返回
/// 返回审批运行状态与服务端重读的正式业务结果。
pub async fn sales_order_review_decide(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<SubmitCardSalesApprovalDecisionCommand>,
) -> Result<SubmitCardSalesApprovalDecisionResult> {
    let db = state.db();
    let (runtime_command, guard) = command.into_runtime_command(actor.id().to_string())?;
    let runtime = state.approval_runtime(Arc::new(CardSalesApprovalActionPort::for_decision(
        db.clone(),
        guard.clone(),
    )));
    let approval = runtime.submit_decision(runtime_command).await?;
    let view = SalesReviewService::new(db)
        .card_sales_decision_result(approval, &guard)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "撤回本人提交的卡券销售审批",
    resource = "sales_order",
    action = "cancel_approval"
)]
/// 撤回尚未形成不可逆决定的卡券销售审批并恢复销售草稿。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的提交人身份
/// * `command` - W05 强类型撤回信封
///
/// # 返回
/// 返回最新审批实例、步骤、可选关闭待办和销售业务结果。
pub async fn sales_order_review_cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<CancelCardSalesApprovalCommand>,
) -> Result<CancelCardSalesApprovalResult> {
    let db = state.db();
    let actor_id = actor.id().to_string();
    let (runtime_command, guard) = command.into_runtime_command(actor_id)?;
    let runtime = state.approval_runtime(Arc::new(CardSalesApprovalActionPort::new(db.clone())));
    let approval = runtime.cancel_approval(runtime_command).await?;
    let view = SalesReviewService::new(db)
        .card_sales_cancel_result(approval, &guard)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
/// * `actor` - 已通过鉴权的当前操作人
/// * `id` - 确认批次 ID
/// * `params` - 正式任务入口参数
///
/// # 返回
/// 返回详情视图。
pub async fn procurement_confirmation_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<ProcurementConfirmationDetailParams>,
) -> Result<ProcurementConfirmationDetailView> {
    let view = SalesReviewService::new(state.db())
        .procurement_confirmation_detail(&id, &params, &actor, state.rbac())
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[allow(dead_code)]
#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "计算采购推荐方案",
    resource = "procurement_confirmation",
    action = "detail"
)]
/// 计算采购二次确认的最低可执行成本方案。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 确认批次 ID
///
/// # 返回
/// 返回按当前供给、能力、数量与起订量计算的推荐分配及采购单草稿分组。
pub async fn procurement_confirmation_recommendation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ProcurementRecommendationView> {
    let view = SalesReviewService::new(state.db())
        .procurement_recommendation(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[allow(dead_code)]
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
/// 返回新的采购确认编辑版本与待办活动版本。
pub async fn procurement_confirmation_save_lines(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveProcurementConfirmationLinesRequest>,
) -> Result<SaveProcurementConfirmationResult> {
    let view = SalesReviewService::new(state.db())
        .save_procurement_confirmation_lines(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[allow(dead_code)]
#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "完成采购确认",
    resource = "procurement_confirmation",
    action = "complete"
)]
/// 以唯一强命令完成采购确认通过或驳回。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 确认批次 ID
/// * `command` - 嵌套正式决定、版本和幂等键
///
/// # 返回
/// 返回已完成待办与真实业务结果；本动作不创建采购单。
pub async fn procurement_confirmation_complete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<CompleteProcurementConfirmationCommand>,
) -> Result<CompleteProcurementConfirmationResult> {
    let view = SalesReviewService::new(state.db())
        .complete_procurement_confirmation(&id, command, &actor, state.rbac())
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
    let rbac = state.rbac();
    let view = SalesReviewService::new(state.db())
        .create_sales_change_order(req, &actor, &rbac)
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
/// 提交销售变更并启动统一审批。客户端不得选择定义或审批人。
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
/// 履约影响确认不得充当审批流程节点，恒失败关闭。
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
/// 财务复核不得充当审批流程节点，恒失败关闭。最终动作仅为 apply_effective_change。
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

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "撤回销售变更审批",
    resource = "sales_change_order",
    action = "submit"
)]
/// 撤回尚未最终通过的销售变更审批，回到可修正草稿。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelSalesChangeApprovalRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .cancel_approval(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
