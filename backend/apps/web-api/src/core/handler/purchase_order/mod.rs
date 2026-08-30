//! 域 D15 `purchase_order` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::purchase_order` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    purchase_order::{
        CancelPurchaseChangeApprovalRequest, CancelPurchaseOrderApprovalRequest,
        CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult,
        CreatePurchaseOrdersFromSourcingRequest, CreatePurchaseOrdersFromSourcingResult,
        CreationBasisListParams, CreationBasisView, EffectPurchaseChangeRequest, PageView,
        PurchaseChangeEffectResult, PurchaseChangeOrderListParams, PurchaseChangeOrderView,
        PurchaseOrderCenterView, PurchaseOrderListItemView, PurchaseOrderListParams, PurchaseOrderService,
        SavePurchaseOrderDraftRequest, SavePurchaseOrderDraftResult, StartPurchaseChangeRequest,
        StartPurchaseChangeResult, SubmitPurchaseChangeRequest, SubmitPurchaseOrderRequest,
        SubmitPurchaseOrderResult, VoidPurchaseOrderRequest, VoidPurchaseOrderResult,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "查询采购单列表",
    resource = "purchase_order",
    action = "list"
)]
/// 查询采购单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn purchase_order_list(
    State(state): State<AppState>,
    Query(params): Query<PurchaseOrderListParams>,
) -> Result<PageView<PurchaseOrderListItemView>> {
    let page = PurchaseOrderService::new(state.db())
        .purchase_order_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "查询采购单对象中心",
    resource = "purchase_order",
    action = "detail"
)]
/// 查询采购单对象中心。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前已认证账号，用于服务端计算审核责任动作
/// * `id` - 采购单 ID
///
/// # 返回
/// 返回对象中心视图。
pub async fn purchase_order_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<PurchaseOrderCenterView> {
    let view = PurchaseOrderService::new(state.db())
        .purchase_order_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "依据采购确认创建采购单",
    resource = "purchase_order",
    action = "create"
)]
/// 依据采购确认创建采购单并提交审批（幂等：同拆单维度结果复用）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ basis_id, purchase_type, payment_term_code, idempotency_key }`）
///
/// # 返回
/// 返回新建（或复用）采购单结果。
pub async fn purchase_order_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePurchaseOrderFromBasisRequest>,
) -> Result<CreatePurchaseOrderResult> {
    let view = PurchaseOrderService::with_rbac(state.db(), state.rbac())
        .create_from_basis(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "确认供给分配",
    resource = "purchase_order",
    action = "create"
)]
/// 一次确认库存与采购供给分配（幂等：同键同载荷回放原结果）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 供给分配请求（销售单、任务、逐行库存或采购依据、数量、幂等键）
///
/// # 返回
/// 返回本次建立或回放的库存预占与采购单。
pub async fn purchase_order_create_from_sourcing(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePurchaseOrdersFromSourcingRequest>,
) -> Result<CreatePurchaseOrdersFromSourcingResult> {
    let view = PurchaseOrderService::with_rbac(state.db(), state.rbac())
        .create_from_sourcing(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "保存采购草稿",
    resource = "purchase_order",
    action = "update"
)]
/// 保存采购草稿（乐观锁：请求携带期望版本，冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 采购单 ID
/// * `req` - 保存请求（表头 + 完整行）
///
/// # 返回
/// 返回新乐观锁版本与表头汇总。
pub async fn purchase_order_save_draft(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SavePurchaseOrderDraftRequest>,
) -> Result<SavePurchaseOrderDraftResult> {
    let view = PurchaseOrderService::with_rbac(state.db(), state.rbac())
        .save_draft(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "作废采购单草稿",
    resource = "purchase_order",
    action = "delete"
)]
/// 作废采购草稿并释放对应销售采购覆盖。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 采购单 ID
/// * `req` - 期望版本、原因和幂等键
///
/// # 返回
/// 返回作废后的状态与乐观锁版本。
pub async fn purchase_order_void(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidPurchaseOrderRequest>,
) -> Result<VoidPurchaseOrderResult> {
    let view = PurchaseOrderService::with_rbac(state.db(), state.rbac())
        .void_draft(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "提交采购单审批",
    resource = "purchase_order",
    action = "submit"
)]
/// 提交采购单并启动统一审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 采购单 ID
/// * `req` - 提交请求（期望版本 + 幂等键）
///
/// # 返回
/// 返回提交结果（提交 ID、序号与审核待办）。
pub async fn purchase_order_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitPurchaseOrderRequest>,
) -> Result<SubmitPurchaseOrderResult> {
    let view = PurchaseOrderService::with_rbac(state.db(), state.rbac())
        .submit(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "撤回采购单审批",
    resource = "purchase_order",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的采购单审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 采购单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 撤回成功返回空数据。
pub async fn purchase_order_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelPurchaseOrderApprovalRequest>,
) -> Result<()> {
    PurchaseOrderService::with_rbac(state.db(), state.rbac())
        .cancel_approval(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "查询供给分配依据",
    resource = "purchase_order",
    action = "create"
)]
/// 查询当前账号开放任务范围内的库存与采购供给依据。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前已认证账号
/// * `params` - 可选销售单和供给分配任务筛选
///
/// # 返回
/// 返回当前账号可处理的精确库存与采购供给依据。
pub async fn purchase_creation_basis_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CreationBasisListParams>,
) -> Result<Vec<CreationBasisView>> {
    let views = PurchaseOrderService::new(state.db())
        .creation_basis_list(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(views))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "发起采购变更",
    resource = "purchase_change_order",
    action = "create"
)]
/// 发起采购变更（基于当前生效版本创建变更单）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 采购单 ID
/// * `req` - 发起请求
///
/// # 返回
/// 返回变更单结果。
pub async fn purchase_change_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<StartPurchaseChangeRequest>,
) -> Result<StartPurchaseChangeResult> {
    let view = PurchaseOrderService::with_rbac(state.db(), state.rbac())
        .start_change(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "提交采购变更审批",
    resource = "purchase_change_order",
    action = "submit"
)]
/// 提交采购变更并启动统一审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 提交请求
///
/// # 返回
/// 返回变更提交结果。
pub async fn purchase_change_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitPurchaseChangeRequest>,
) -> Result<PurchaseChangeOrderView> {
    let view = PurchaseOrderService::with_rbac(state.db(), state.rbac())
        .submit_change_view(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "采购变更生效",
    resource = "purchase_change_order",
    action = "post"
)]
/// 客户端直接生效失败关闭。最终动作仅由审批运行时 `on_final_approve` 调用。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 生效请求（客户端不得据此改写采购单）
///
/// # 返回
/// 恒返回冲突。
pub async fn purchase_change_effect(
    State(_state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<EffectPurchaseChangeRequest>,
) -> Result<PurchaseChangeEffectResult> {
    match PurchaseOrderService::reject_client_effect() {
        Err(error) => Err(error.into()),
        Ok(result) => Ok(ApiResponse::ok_with_data(result)),
    }
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "撤回采购变更审批",
    resource = "purchase_change_order",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的采购变更审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 撤回成功返回空数据。
pub async fn purchase_change_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelPurchaseChangeApprovalRequest>,
) -> Result<()> {
    PurchaseOrderService::with_rbac(state.db(), state.rbac())
        .cancel_change_approval(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "查询采购变更单列表",
    resource = "purchase_change_order",
    action = "list"
)]
/// 查询采购变更单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn purchase_change_list(
    State(state): State<AppState>,
    Query(params): Query<PurchaseChangeOrderListParams>,
) -> Result<PageView<PurchaseChangeOrderView>> {
    let page = PurchaseOrderService::new(state.db())
        .change_order_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "查询采购变更单详情",
    resource = "purchase_change_order",
    action = "detail"
)]
/// 查询采购变更单详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 变更单 ID
///
/// # 返回
/// 返回变更单视图。
pub async fn purchase_change_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<PurchaseChangeOrderView> {
    let view = PurchaseOrderService::new(state.db())
        .change_order_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[cfg(test)]
mod tests {
    /// 采购变更 HTTP 只走统一提交、撤回、生效与详情，客户端不得选定义。
    #[test]
    fn purchase_change_http_uses_unified_ports() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("submit_change"));
        assert!(production.contains("cancel_change_approval"));
        assert!(production.contains("reject_client_effect"));
        assert!(production.contains("change_order_detail"));
        assert!(production.contains("with_rbac"));
        assert!(!production.contains(".apply_effective_change("));
        assert!(!production.contains("definition_id"));
        assert!(!production.contains("PENDING_WAREHOUSE_IMPACT"));
        assert!(!production.contains("PENDING_FINANCE_REVIEW"));
    }
}
