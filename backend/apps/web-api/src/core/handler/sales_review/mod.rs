//! 域 D14 `sales_review` 的 HTTP handler。
//!
//! 仅保留销售变更单生命周期。卡券专用审批、采购确认与低毛利入口已删除。

use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_processes::sales_change::SalesChangeProcess;
use erp_read_models::sales_center::review::{
    SalesChangeListView, SalesChangeOrderDetailView, SalesChangeReadService,
};
use erp_sales::dto::sales_review::{
    CancelSalesChangeApprovalRequest, CreateSalesChangeOrderRequest, SalesChangeOrderListParams,
    SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

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
/// * `actor` - 已认证操作人
/// * `query` - 分页与筛选参数（`sales_order_id`/`status` 扁平传递，含跨页 `scope_version`）
///
/// # 返回
/// 返回带范围版本的分页视图。
///
/// # 错误
/// 无动作权限、范围变化、筛选非法或仓储失败时拒绝。
///
/// # 关键业务约束
/// 沿来源销售单责任接入；缺范围返回空集并标记 `no_scope`。
pub async fn sales_change_order_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SalesChangeOrderListParams>,
) -> Result<SalesChangeListView> {
    let page = SalesChangeReadService::with_rbac(state.db(), state.rbac())
        .sales_change_order_list(&params, &actor)
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
/// * `actor` - 已认证操作人
/// * `id` - 变更单 ID
///
/// # 返回
/// 返回详情视图。
///
/// # 错误
/// 无动作权限、不可见或不存在时拒绝。
///
/// # 关键业务约束
/// 列表已授权不能作为详情凭证；沿来源销售单 detail 动作重验。
pub async fn sales_change_order_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesChangeReadService::with_rbac(state.db(), state.rbac())
        .sales_change_order_detail(&id, &actor)
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
    let view = SalesChangeProcess::new(state.db(), state.rbac())
        .with_object_read(state.approval_object_read())
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
    let view = SalesChangeProcess::new(state.db(), state.rbac())
        .with_object_read(state.approval_object_read())
        .submit_sales_change(&id, req, &actor)
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
    let view = SalesChangeProcess::new(state.db(), state.rbac())
        .with_object_read(state.approval_object_read())
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
    let view = SalesChangeProcess::new(state.db(), state.rbac())
        .with_object_read(state.approval_object_read())
        .cancel_approval(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
