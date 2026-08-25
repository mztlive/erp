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
        CancelCustomerReceiptApprovalRequest, CardFundsRegistrationResult, CardFundsReviewDetailParams,
        CommitCustomerReceiptRequest, CommitInvoiceRequest, CommitRedInvoiceRequest,
        CompleteCardFundsReviewCommand, CompleteCardFundsReviewResult, CreateCustomerReceiptRequest,
        CreateInvoiceRequest, CreateReceivableAccountRequest, CustomerReceiptListParams, CustomerReceiptView,
        InvoiceListParams, InvoiceView, PageView, PostCustomerReceiptRequest, PostInvoiceRequest,
        ReceivableAccountListParams, ReceivableAccountView, ReceivableService,
        RegisterCardFundsInvoiceRequest, RegisterCardFundsReceiptRequest, SubmitCustomerReceiptRequest,
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
    desc = "原子登记卡券票款历史回款",
    resource = "customer_receipt",
    action = "create"
)]
/// 在当前 W13 正式任务内一次登记历史回款、核销分配与进度。
pub async fn card_funds_receipt_register(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterCardFundsReceiptRequest>,
) -> Result<CardFundsRegistrationResult> {
    let result = ReceivableService::new(state.db())
        .register_card_funds_receipt(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "原子登记卡券票款历史发票",
    resource = "invoice",
    action = "create"
)]
/// 在当前 W13 正式任务内一次登记历史销项发票、分配与进度。
pub async fn card_funds_invoice_register(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterCardFundsInvoiceRequest>,
) -> Result<CardFundsRegistrationResult> {
    let result = ReceivableService::new(state.db())
        .register_card_funds_invoice(req, &actor)
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
    desc = "原子创建或提交客户回款审批",
    resource = "customer_receipt",
    action = "submit"
)]
/// 原子创建或提交客户回款并启动统一审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 新回款或已有草稿、冻结分配与幂等键
///
/// # 返回
/// 返回进入审批后的回款单视图。
pub async fn customer_receipt_commit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableService::new(state.db())
        .commit_customer_receipt(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "提交客户回款审批",
    resource = "customer_receipt",
    action = "submit"
)]
/// 提交客户回款并启动统一审批。客户端不得选择定义或审批人。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 回款单 ID
/// * `req` - 提交请求（版本、幂等键与冻结分配）
///
/// # 返回
/// 返回提交后的回款单视图。
pub async fn customer_receipt_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableService::new(state.db())
        .submit_customer_receipt(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款、销项发票与卡券票款复核管理（W11/W13）",
    desc = "撤回客户回款审批",
    resource = "customer_receipt",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的客户回款审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 回款单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回撤回后的回款单视图。
pub async fn customer_receipt_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelCustomerReceiptApprovalRequest>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableService::new(state.db())
        .cancel_customer_receipt_approval(&id, req, &actor)
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
/// 客户端直接过账失败关闭。过账只允许作为审批最终通过动作。
///
/// # 参数
/// * `_state` - 应用状态
/// * `_actor` - 已通过鉴权的审计操作人
/// * `_id` - 回款单 ID
/// * `_req` - 过账请求（客户端不得据此形成资金事实）
///
/// # 错误
/// 始终返回冲突，防止 HTTP 旁路过账。
pub async fn customer_receipt_post(
    State(_state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    match ReceivableService::reject_client_post() {
        Err(error) => Err(error.into()),
        Ok(result) => Ok(ApiResponse::ok_with_data(result)),
    }
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
/// 登记发票草稿。不绑定审批定义，不启动审批实例，不创建审批任务。
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
    desc = "原子登记销项发票并分配",
    resource = "invoice",
    action = "post"
)]
/// 原子创建或提交销项发票并完成正式分配。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 新发票或已有草稿、分配与乐观锁版本
///
/// # 返回
/// 返回登记后的发票视图。
pub async fn invoice_commit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitInvoiceRequest>,
) -> Result<InvoiceView> {
    let view = ReceivableService::new(state.db())
        .commit_invoice(req, &actor)
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
    Json(req): Json<CommitRedInvoiceRequest>,
) -> Result<InvoiceView> {
    let view = ReceivableService::new(state.db())
        .issue_red_invoice(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[cfg(test)]
mod tests {
    use services::receivable::{CreateInvoiceRequest, SubmitCustomerReceiptRequest};

    /// 发票 HTTP 只暴露创建/过账/红冲，不得提交审批或选择定义。
    #[test]
    fn invoice_http_proves_no_approval() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("create_invoice"));
        assert!(production.contains("invoice_create"));
        assert!(production.contains("invoice_detail"));
        assert!(production.contains("invoice_post"));
        assert!(!production.contains("submit_invoice"));
        assert!(!production.contains("invoice_submit"));
        assert!(!production.contains("cancel_invoice"));
        assert!(!production.contains("invoice_cancel"));
        assert!(!production.contains("start_invoice"));
        assert!(!production.contains("InvoiceAdapter"));
        let invoice_create = production
            .split("pub async fn invoice_create")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn invoice_post").next())
            .expect("invoice_create 生产片段");
        assert!(invoice_create.contains("create_invoice"));
        assert!(!invoice_create.contains("submit_"));
        assert!(!invoice_create.contains("start_approval"));
        assert!(!invoice_create.contains("definition_id"));
        assert!(serde_json::from_value::<CreateInvoiceRequest>(serde_json::json!({
            "invoice_direction": "sales",
            "invoice_kind": "blue",
            "party_id": "p-1",
            "invoice_no": "001",
            "invoice_date": "2026-08-06",
            "gross_amount": "100.00",
            "net_amount": "88.50",
            "tax_amount": "11.50",
            "definition_id": "forged",
            "assignee": "forged"
        }))
        .is_err());
    }

    /// 客户回款 HTTP 只走统一提交、撤回与详情，客户端不得选定义或直接过账。
    #[test]
    fn customer_receipt_http_uses_unified_ports() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("submit_customer_receipt"));
        assert!(production.contains("cancel_customer_receipt_approval"));
        assert!(production.contains("reject_client_post"));
        assert!(production.contains("customer_receipt_detail"));
        assert!(!production.contains(".post_customer_receipt("));
        assert!(!production.contains("definition_id"));
        assert!(!production.contains("PENDING_REVIEW"));
        assert!(
            serde_json::from_value::<SubmitCustomerReceiptRequest>(serde_json::json!({
                "expected_version": 1,
                "idempotency_key": "k1",
                "allocations": [{"receivable_entry_id": "re-1", "allocated_amount": "10"}],
                "assignee": "forged"
            }))
            .is_err()
        );
    }
}
