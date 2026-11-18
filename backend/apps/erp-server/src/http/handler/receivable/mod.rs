//! 域 D18 `receivable` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_finance::dto::receivable` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use crate::wiring::receivable_posting_ports;
pub mod allocation_source;
pub(crate) mod invoice_files;
pub(crate) mod receipt_files;

pub mod draft_read;
pub mod draft_update;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_finance::dto::receivable::{
    CancelCustomerReceiptApprovalRequest, CommitCustomerReceiptRequest, CommitInvoiceRequest,
    CommitRedInvoiceRequest, CreateCustomerReceiptRequest, CreateInvoiceRequest,
    CreateReceivableAccountRequest, CustomerReceiptListParams, InvoiceListParams, InvoiceView,
    PostCustomerReceiptRequest, PostInvoiceRequest, ReceivableAccountListParams,
    SubmitCustomerReceiptRequest,
};
use erp_finance_posting::finance_posting::receivable::ReceivableProcess;
use erp_read_models::finance::dto::{CustomerReceiptView, ReceivableAccountView};
use erp_read_models::finance::funds_scope::{
    FundsScopedPage, FundsScopedResult, ScopedCustomerReceiptRow, ScopedInvoiceRow,
    ScopedReceivableAccountRow,
};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 按请求条件查询应收往来子账列表。
///
/// DataScope v2：按关联销售单当前负责人与登记经办人分别查询，同一事务快照；
/// 部分授权仅返获授权份额，整单金额为 null，跨页原样回传 `scope_version`。
/// 返回范围分页视图（`items`/`total`/`summary`/`scope_version`）。负责销售候选不随列表返回。
pub async fn receivable_account_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ReceivableAccountListParams>,
) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
    let page = crate::wiring::funds_access(state.application().db(), state.application().authorization())
        .receivable_account_list_scoped(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询应收往来子账详情（DataScope v2 范围裁剪行）。
/// 返回范围裁剪后的台账行；不可见与不存在统一为 NotFound。
pub async fn receivable_account_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FundsScopedResult<ScopedReceivableAccountRow>> {
    let view = crate::wiring::funds_access(state.application().db(), state.application().authorization())
        .receivable_account_detail_scoped(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 建立应收往来子账与原始应收分录（跨集合事务）。
/// 返回新建子账的响应视图。
pub async fn receivable_account_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateReceivableAccountRequest>,
) -> Result<ReceivableAccountView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .create_receivable_account(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按请求条件查询客户回款单列表。
///
/// DataScope v2：按核销关联销售当前负责人与登记/核销经办人分别查询；
/// 回款金额按分配事实只算匹配份额，未分配单列。
/// 返回范围分页视图（`items`/`total`/`summary`/`scope_version`）。负责销售候选不随列表返回。
pub async fn customer_receipt_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CustomerReceiptListParams>,
) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
    let page = crate::wiring::funds_access(state.application().db(), state.application().authorization())
        .customer_receipt_list_scoped(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询客户回款单详情（DataScope v2 范围裁剪行）。
/// 返回范围裁剪后的回款行；不可见与不存在统一为 NotFound。
pub async fn customer_receipt_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FundsScopedResult<ScopedCustomerReceiptRow>> {
    let view = crate::wiring::funds_access(state.application().db(), state.application().authorization())
        .customer_receipt_detail_scoped(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记客户回款草稿（回款单号唯一构成幂等去重）。
/// 返回新建回款单视图。
pub async fn customer_receipt_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .create_customer_receipt(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 原子创建或提交客户回款并启动统一审批。
/// `req` 为新回款或已有草稿、冻结分配与幂等键。
/// 返回进入审批后的回款单视图。
pub async fn customer_receipt_commit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .commit_customer_receipt(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 提交客户回款并启动统一审批。客户端不得选择定义或审批人。
/// `req` 为提交请求（版本、幂等键与冻结分配）。
/// 返回提交后的回款单视图。
pub async fn customer_receipt_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .submit_customer_receipt(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 撤回尚未最终通过的客户回款审批。
/// `req` 为撤回请求（原因必填）。
/// 返回撤回后的回款单视图。
pub async fn customer_receipt_cancel_approval(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelCustomerReceiptApprovalRequest>,
) -> Result<CustomerReceiptView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .cancel_customer_receipt_approval(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 保留旧回款直接过账入口但恒返回冲突，不从客户端请求形成资金事实。回款只在审批最终通过时过账。
pub async fn customer_receipt_post(
    State(_state): State<HttpState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<PostCustomerReceiptRequest>,
) -> Result<CustomerReceiptView> {
    match ReceivableProcess::reject_client_post() {
        Err(error) => Err(error.into()),
        Ok(result) => Ok(ApiResponse::ok_with_data(result)),
    }
}

/// 查询发票列表（销项/进项共用，`invoice_direction` 筛选）。
///
/// DataScope v2：销项按负责销售与登记经办，进项按采购负责人与登记经办；
/// 双方向同一事务快照，部分授权仅返获授权份额。
/// 返回范围分页视图（`items`/`total`/`summary`/`scope_version`）。销售与采购负责人候选不合并进列表。
pub async fn invoice_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<InvoiceListParams>,
) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
    let page = crate::wiring::funds_access(state.application().db(), state.application().authorization())
        .invoice_list_scoped(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询发票详情（DataScope v2 范围裁剪行）。
/// 返回范围裁剪后的发票行；不可见与不存在统一为 NotFound。
pub async fn invoice_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FundsScopedResult<ScopedInvoiceRow>> {
    let view = crate::wiring::funds_access(state.application().db(), state.application().authorization())
        .invoice_detail_scoped(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 登记发票草稿。不绑定审批定义，不启动审批实例，不创建审批任务。
/// 返回新建发票视图。
pub async fn invoice_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateInvoiceRequest>,
) -> Result<InvoiceView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .create_invoice(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 原子创建或提交销项发票并完成正式分配。
/// `req` 为新发票或已有草稿、分配与乐观锁版本。
/// 返回登记后的发票视图。
pub async fn invoice_commit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CommitInvoiceRequest>,
) -> Result<InvoiceView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .commit_invoice(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 发票登记过账并分配（§8.3-2 事务不变量，资金入口幂等去重）。
/// 返回登记后发票视图。
pub async fn invoice_post(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostInvoiceRequest>,
) -> Result<InvoiceView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .post_invoice(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 开具红票并红冲（§8.3-3 事务不变量：保留原事实，累计红冲不超原分配）。
/// 返回新建红票视图。
pub async fn invoice_red_issue(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CommitRedInvoiceRequest>,
) -> Result<InvoiceView> {
    let view = ReceivableProcess::new(
        state.application().db(),
        receivable_posting_ports(state.application().db(), state.application().authorization()),
    )
    .issue_red_invoice(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

pub mod invoice_request;
