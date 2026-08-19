//! 域 D13 `sales_order` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::sales_order` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    sales_order::{
        CancelSalesOrderApprovalRequest, CreateSalesOrderRequest, PageView, SalesOrderDetailView,
        SalesOrderListParams, SalesOrderService, SalesOrderView, SaveWorkingCopyRequest, SubmissionView,
        SubmitSalesOrderRequest, VoidSalesOrderRequest, WorkingCopyView,
    },
};

use crate::{
    app_state::AppState,
    core::{
        errors::Result, extractor::UserID, handler::customer::ensure_customer_access,
        middleware::RbacSubject, response::ApiResponse,
    },
};

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "查询销售单列表",
    resource = "sales_order",
    action = "list"
)]
/// 查询销售单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn sales_order_list(
    State(state): State<AppState>,
    Query(params): Query<SalesOrderListParams>,
) -> Result<PageView<SalesOrderView>> {
    let page = SalesOrderService::new(state.db())
        .sales_order_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "创建销售单",
    resource = "sales_order",
    action = "create"
)]
/// 创建销售单（订单 + 稳定明细 + 工作副本原子形成；`intent=SUBMIT` 时立即提交）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（含幂等键与意图）
///
/// # 返回
/// 返回销售单详情视图。
pub async fn sales_order_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Json(req): Json<CreateSalesOrderRequest>,
) -> Result<SalesOrderDetailView> {
    ensure_customer_access(&state, &subject, &user_id, &req.customer_id).await?;
    let view = SalesOrderService::with_rbac(state.db(), state.rbac())
        .create_sales_order(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "查询销售单详情",
    resource = "sales_order",
    action = "detail"
)]
/// 查询销售单详情（订单 + 稳定明细 + 草稿 + 提交历史 + 版本历史）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 销售单 ID
///
/// # 返回
/// 返回详情视图。
pub async fn sales_order_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderService::with_rbac(state.db(), state.rbac())
        .sales_order_detail(&id, Some(&actor))
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "保存销售单草稿",
    resource = "sales_order",
    action = "update"
)]
/// 保存草稿（整表头覆盖 + 明细整批替换，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单 ID
/// * `req` - 保存请求（含期望版本）
///
/// # 返回
/// 返回保存后的工作副本视图。
pub async fn sales_order_save_working_copy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveWorkingCopyRequest>,
) -> Result<WorkingCopyView> {
    let view = SalesOrderService::new(state.db())
        .save_working_copy(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "提交销售单",
    resource = "sales_order",
    action = "submit"
)]
/// 提交销售单（冻结提交快照并推进审核轨；重复提交幂等返回既有提交）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单 ID
/// * `req` - 提交请求（含期望版本与幂等键）
///
/// # 返回
/// 返回提交快照视图。
pub async fn sales_order_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSalesOrderRequest>,
) -> Result<SubmissionView> {
    let view = SalesOrderService::with_rbac(state.db(), state.rbac())
        .submit_sales_order(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "撤回销售单审批",
    resource = "sales_order",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的销售单审批（含 `VoucherSalesOrder`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回撤回后的销售单详情。
pub async fn sales_order_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelSalesOrderApprovalRequest>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderService::with_rbac(state.db(), state.rbac())
        .cancel_approval_submission(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "作废销售单草稿",
    resource = "sales_order",
    action = "delete"
)]
/// 作废销售单草稿（主状态 `DRAFT → VOIDED`，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单 ID
/// * `req` - 作废请求（含期望版本）
///
/// # 返回
/// 返回作废后的销售单详情视图。
pub async fn sales_order_void(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidSalesOrderRequest>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderService::new(state.db())
        .void_sales_order(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[cfg(test)]
mod tests {
    /// HTTP 调用方对卡券销售单只走统一提交/撤回，不得新增专用决定入口。
    #[test]
    fn voucher_sales_order_http_uses_unified_ports() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("submit_sales_order"));
        assert!(production.contains("cancel_approval_submission"));
        assert!(production.contains("VoucherSalesOrder"));
        assert!(!production.contains("CARD_SALES_APPROVAL"));
        assert!(!production.contains("CardSalesManagerApproval"));
        assert!(!production.contains("CardSalesOperationApproval"));
        assert!(!production.contains("InternalApprovalRuntime"));
    }
}
