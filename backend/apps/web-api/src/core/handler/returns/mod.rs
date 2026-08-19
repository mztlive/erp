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
        CancelCustomerRefundApprovalRequest, CancelSupplierRefundApprovalRequest,
        CreateCustomerRefundRequest, CreatePaymentReversalRequest, CreatePurchaseReturnOrderRequest,
        CreateReceiptReversalRequest, CreateSalesReturnCaseRequest, CreateSupplierRefundRequest,
        CustomerRefundListParams, CustomerRefundView, PageView, PaymentReversalView,
        PostCustomerRefundRequest, PostPaymentReversalRequest, PostReceiptReversalRequest,
        PostSupplierRefundRequest, PurchaseReturnOrderListParams, PurchaseReturnOrderView,
        ReceiptReversalView, ReturnsService, SalesReturnCaseListParams, SalesReturnCaseView,
        SubmitCustomerRefundRequest, SubmitSupplierRefundRequest, SupplierRefundView,
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
    desc = "提交客户退款审批",
    resource = "customer_refund",
    action = "submit"
)]
/// 提交客户退款并启动统一审批。客户端不得选择定义或审批人。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 退款单 ID
/// * `req` - 提交请求（版本与幂等键）
///
/// # 返回
/// 返回提交后的退款单视图。
pub async fn customer_refund_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitCustomerRefundRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsService::new(state.db())
        .submit_customer_refund(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "撤回客户退款审批",
    resource = "customer_refund",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的客户退款审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 退款单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回撤回后的退款单视图。
pub async fn customer_refund_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelCustomerRefundApprovalRequest>,
) -> Result<CustomerRefundView> {
    let view = ReturnsService::new(state.db())
        .cancel_customer_refund_approval(&id, req, &actor)
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
/// 客户端直接过账失败关闭。过账只允许作为审批最终通过动作。
///
/// # 参数
/// * `_state` - 应用状态
/// * `_actor` - 已通过鉴权的审计操作人
/// * `_id` - 退款单 ID
/// * `_req` - 过账请求（客户端不得据此形成资金事实）
///
/// # 错误
/// 始终返回冲突，防止 HTTP 旁路过账。
pub async fn customer_refund_post(
    State(_state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostCustomerRefundRequest>,
) -> Result<CustomerRefundView> {
    match ReturnsService::reject_client_post() {
        Err(error) => Err(error.into()),
        Ok(result) => Ok(ApiResponse::ok_with_data(result)),
    }
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "查询供应商退款详情",
    resource = "supplier_refund",
    action = "detail"
)]
/// 查询供应商退款详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 退款单 ID
///
/// # 返回
/// 返回退款单视图。
pub async fn supplier_refund_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SupplierRefundView> {
    let view = ReturnsService::new(state.db())
        .supplier_refund_detail(&id)
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
    desc = "提交供应商退款审批",
    resource = "supplier_refund",
    action = "submit"
)]
/// 提交供应商退款并启动统一审批。客户端不得选择定义或审批人。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 退款单 ID
/// * `req` - 提交请求（版本与幂等键）
///
/// # 返回
/// 返回提交后的退款单视图。
pub async fn supplier_refund_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsService::new(state.db())
        .submit_supplier_refund(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "退货退款",
    group_desc = "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
    desc = "撤回供应商退款审批",
    resource = "supplier_refund",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的供应商退款审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 退款单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回撤回后的退款单视图。
pub async fn supplier_refund_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelSupplierRefundApprovalRequest>,
) -> Result<SupplierRefundView> {
    let view = ReturnsService::new(state.db())
        .cancel_supplier_refund_approval(&id, req, &actor)
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
/// 客户端直接过账失败关闭。过账只允许作为审批最终通过动作。
///
/// # 参数
/// * `_state` - 应用状态
/// * `_actor` - 已通过鉴权的审计操作人
/// * `_id` - 退款单 ID
/// * `_req` - 过账请求（客户端不得据此形成资金事实）
///
/// # 错误
/// 始终返回冲突，防止 HTTP 旁路过账。
pub async fn supplier_refund_post(
    State(_state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostSupplierRefundRequest>,
) -> Result<SupplierRefundView> {
    match ReturnsService::reject_supplier_refund_client_post() {
        Err(error) => Err(error.into()),
        Ok(result) => Ok(ApiResponse::ok_with_data(result)),
    }
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

#[cfg(test)]
mod tests {
    use services::returns::{SubmitCustomerRefundRequest, SubmitSupplierRefundRequest};

    /// 客户退款 HTTP 只走统一提交、撤回与详情，客户端不得选定义或直接过账。
    #[test]
    fn customer_refund_http_uses_unified_ports() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("submit_customer_refund"));
        assert!(production.contains("cancel_customer_refund_approval"));
        assert!(production.contains("reject_client_post"));
        assert!(production.contains("customer_refund_detail"));
        assert!(!production.contains(".post_customer_refund("));
        assert!(!production.contains("definition_id"));
        assert!(!production.contains("PENDING_REVIEW"));
        assert!(
            serde_json::from_value::<SubmitCustomerRefundRequest>(serde_json::json!({
                "expected_version": 1,
                "idempotency_key": "k1",
                "assignee": "forged"
            }))
            .is_err()
        );
    }

    /// 供应商退款 HTTP 只走统一提交、撤回与详情，客户端不得选定义或直接过账。
    #[test]
    fn supplier_refund_http_uses_unified_ports() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("submit_supplier_refund"));
        assert!(production.contains("cancel_supplier_refund_approval"));
        assert!(production.contains("reject_supplier_refund_client_post"));
        assert!(production.contains("supplier_refund_detail"));
        assert!(!production.contains(".post_supplier_refund("));
        assert!(!production.contains("PENDING_REVIEW"));
        assert!(
            serde_json::from_value::<SubmitSupplierRefundRequest>(serde_json::json!({
                "expected_version": 1,
                "idempotency_key": "k1",
                "assignee": "forged"
            }))
            .is_err()
        );
    }
}
