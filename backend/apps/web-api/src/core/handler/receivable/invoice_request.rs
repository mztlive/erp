//! 开票申请 HTTP 协议适配；复用拥有领域及读模型 DTO。
use erp_finance::dto::receivable::{
    CancelInvoiceRequest, InvoiceRequestAmounts, InvoiceRequestQuery, SubmitInvoiceRequest,
};
use erp_read_models::finance::funds_scope::{FundsScopedPage, FundsScopedResult, ScopedInvoiceRequestRow};
use erp_read_models::finance::receivable::ReceivableReadService;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;

use super::*;

#[permission_macros::permission(
    group = "开票申请",
    group_desc = "开票申请与授权管理",
    desc = "查询开票申请列表",
    resource = "sales_invoice_request",
    action = "list"
)]
/// 查询开票申请列表。
///
/// DataScope v2：按关联销售当前负责人、申请人与当前开票处理人分别查询；
/// 不改变正式开票准入，申请金额为行级事实始终返回。
/// # 返回
/// 返回范围分页视图（`items`/`total`/`summary`/`owner_options`/`scope_version`）。
pub async fn invoice_request_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<InvoiceRequestQuery>,
) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
    Ok(ApiResponse::ok_with_data(
        erp_processes::adapters::funds_access_with_rbac(state.db(), state.rbac())
            .request_list_scoped(&params, &actor)
            .await?,
    ))
}

#[permission_macros::permission(
    group = "开票申请",
    group_desc = "开票申请与授权管理",
    desc = "查询开票申请详情",
    resource = "sales_invoice_request",
    action = "detail"
)]
/// 查询开票申请详情（DataScope v2 范围裁剪行）。
/// # 返回
/// 返回范围裁剪后的申请行；不可见与不存在统一为 NotFound。
pub async fn invoice_request_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FundsScopedResult<ScopedInvoiceRequestRow>> {
    Ok(ApiResponse::ok_with_data(
        erp_processes::adapters::funds_access_with_rbac(state.db(), state.rbac())
            .request_detail_scoped(&id, &actor)
            .await?,
    ))
}

#[permission_macros::permission(
    group = "开票申请",
    group_desc = "开票申请与授权管理",
    desc = "查询可申请开票额度",
    resource = "sales_invoice_request",
    action = "list"
)]
/// 查询可申请开票额度。
/// # 返回
/// 返回已授权业务用例的结果；失败保留既有错误类型。
pub async fn invoice_request_amounts(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<InvoiceRequestAmounts> {
    Ok(ApiResponse::ok_with_data(ReceivableReadService::new(state.db()).invoice_request_amounts(&id).await?))
}

#[permission_macros::permission(
    group = "开票申请",
    group_desc = "开票申请与授权管理",
    desc = "提交开票申请",
    resource = "sales_invoice_request",
    action = "submit"
)]
/// 提交开票申请。
/// # 返回
/// 返回已授权业务用例的结果；失败保留既有错误类型。
pub async fn invoice_request_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<SubmitInvoiceRequest>,
) -> Result<InvoiceRequestView> {
    Ok(ApiResponse::ok_with_data(
        ReceivableProcess::new(state.db())
            .with_object_read(state.approval_object_read())
            .submit_invoice_request(req, &actor)
            .await?,
    ))
}

#[permission_macros::permission(
    group = "开票申请",
    group_desc = "开票申请与授权管理",
    desc = "撤回开票申请",
    resource = "sales_invoice_request",
    action = "cancel"
)]
/// 撤回开票申请。
/// # 返回
/// 返回已授权业务用例的结果；失败保留既有错误类型。
pub async fn invoice_request_cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelInvoiceRequest>,
) -> Result<InvoiceRequestView> {
    Ok(ApiResponse::ok_with_data(
        ReceivableProcess::new(state.db())
            .with_object_read(state.approval_object_read())
            .cancel_invoice_request(&id, req, &actor)
            .await?,
    ))
}
