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
        CancelSupplierPaymentApprovalRequest, CommitSupplierPaymentRequest, CreatePayableAccountRequest,
        CreateSupplierPaymentRequest, PageView, PayableAccountListParams, PayableAccountView, PayableService,
        PostSupplierPaymentRequest, PurchaseInvoiceAllocationListParams, PurchaseInvoiceAllocationView,
        PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest, SubmitSupplierPaymentRequest,
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
    desc = "原子创建或提交供应商付款审批",
    resource = "supplier_payment",
    action = "submit"
)]
/// 原子创建或提交供应商付款并启动统一审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 新付款或已有草稿、冻结分配与幂等键
///
/// # 返回
/// 返回进入审批后的付款单视图。
pub async fn supplier_payment_commit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitSupplierPaymentRequest>,
) -> Result<SupplierPaymentView> {
    let view = PayableService::new(state.db())
        .commit_supplier_payment(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "提交供应商付款审批",
    resource = "supplier_payment",
    action = "submit"
)]
/// 提交供应商付款并启动统一审批。客户端不得选择定义或审批人。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 付款单 ID
/// * `req` - 提交请求（版本、幂等键与冻结分配）
///
/// # 返回
/// 返回提交后的付款单视图。
pub async fn supplier_payment_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSupplierPaymentRequest>,
) -> Result<SupplierPaymentView> {
    let view = PayableService::new(state.db())
        .submit_supplier_payment(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "撤回供应商付款审批",
    resource = "supplier_payment",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的供应商付款审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 付款单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回撤回后的付款单视图。
pub async fn supplier_payment_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelSupplierPaymentApprovalRequest>,
) -> Result<SupplierPaymentView> {
    let view = PayableService::new(state.db())
        .cancel_supplier_payment_approval(&id, req, &actor)
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
/// 客户端直接过账失败关闭。过账只允许作为审批最终通过动作。
///
/// # 参数
/// * `_state` - 应用状态
/// * `_actor` - 已通过鉴权的审计操作人
/// * `_id` - 付款单 ID
/// * `_req` - 过账请求（客户端不得据此形成资金事实）
///
/// # 错误
/// 始终返回冲突，防止 HTTP 旁路过账。
pub async fn supplier_payment_post(
    State(_state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostSupplierPaymentRequest>,
) -> Result<SupplierPaymentView> {
    match PayableService::reject_client_post() {
        Err(error) => Err(error.into()),
        Ok(result) => Ok(ApiResponse::ok_with_data(result)),
    }
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

#[cfg(test)]
mod tests {
    use services::payable::SubmitSupplierPaymentRequest;

    /// 供应商付款 HTTP 只走统一提交、撤回与详情，客户端不得选定义或直接过账。
    #[test]
    fn supplier_payment_http_uses_unified_ports() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("submit_supplier_payment"));
        assert!(production.contains("cancel_supplier_payment_approval"));
        assert!(production.contains("reject_client_post"));
        assert!(production.contains("supplier_payment_detail"));
        assert!(!production.contains(".post_supplier_payment("));
        assert!(!production.contains("definition_id"));
        assert!(!production.contains("PENDING_REVIEW"));
        assert!(
            serde_json::from_value::<SubmitSupplierPaymentRequest>(serde_json::json!({
                "expected_version": 1,
                "idempotency_key": "k1",
                "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10"}],
                "assignee": "forged"
            }))
            .is_err()
        );
    }
}
