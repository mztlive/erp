//! 开票申请 HTTP 协议适配；复用拥有领域及读模型 DTO。
use erp_finance::dto::receivable::{
    CancelInvoiceRequest, InvoiceRequestAmounts, InvoiceRequestQuery, SubmitInvoiceRequest,
    UpdateInvoiceRequest,
};
use erp_read_models::finance::funds_scope::{FundsScopedPage, FundsScopedResult, ScopedInvoiceRequestRow};
use erp_read_models::finance::receivable::ReceivableReadService;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;

use super::*;
use crate::wiring::receivable_posting_ports;

/// 查询开票申请列表。
///
/// DataScope v2：按关联销售当前负责人、申请人与当前开票处理人分别查询；
/// 不改变正式开票准入，申请金额为行级事实始终返回。
/// # 返回
/// 返回范围分页视图（`items`/`total`/`summary`/`scope_version`）。负责销售候选不随列表返回。
pub async fn invoice_request_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<InvoiceRequestQuery>,
) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
    Ok(ApiResponse::ok_with_data(
        crate::wiring::funds_access_with_rbac(state.application().db(), state.application().rbac())
            .request_list_scoped(&params, &actor)
            .await?,
    ))
}

/// 查询开票申请详情（DataScope v2 范围裁剪行）。
/// # 返回
/// 返回范围裁剪后的申请行；不可见与不存在统一为 NotFound。
pub async fn invoice_request_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FundsScopedResult<ScopedInvoiceRequestRow>> {
    Ok(ApiResponse::ok_with_data(
        crate::wiring::funds_access_with_rbac(state.application().db(), state.application().rbac())
            .request_detail_scoped(&id, &actor)
            .await?,
    ))
}

/// 查询可申请开票额度。
/// # 返回
/// 返回已授权业务用例的结果；失败保留既有错误类型。
pub async fn invoice_request_amounts(
    State(state): State<HttpState>,
    Path(id): Path<String>,
) -> Result<InvoiceRequestAmounts> {
    Ok(ApiResponse::ok_with_data(
        ReceivableReadService::new(state.application().db()).invoice_request_amounts(&id).await?,
    ))
}

/// 提交开票申请。
/// # 返回
/// 返回已授权业务用例的结果；失败保留既有错误类型。
pub async fn invoice_request_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<SubmitInvoiceRequest>,
) -> Result<InvoiceRequestView> {
    Ok(ApiResponse::ok_with_data(
        ReceivableProcess::new(state.application().db(), receivable_posting_ports(state.application().db()))
            .submit_invoice_request(req, &actor)
            .await?,
    ))
}

/// 保存草稿或已驳回开票申请的修改。
/// # 参数
/// * `state` - 应用资源。
/// * `actor` - 已认证的原申请人。
/// * `id` - 当前申请。
/// * `req` - 当前版本与待保存内容。
/// # 返回
/// 返回保存后的申请；已驳回状态保持，保存不占用开票额度。
/// # 错误
/// 权限、原申请人、版本、状态或内容不合法时拒绝。
pub async fn invoice_request_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateInvoiceRequest>,
) -> Result<InvoiceRequestView> {
    Ok(ApiResponse::ok_with_data(
        ReceivableProcess::new(state.application().db(), receivable_posting_ports(state.application().db()))
            .update_invoice_request(&id, req, &actor)
            .await?,
    ))
}

/// 撤回开票申请。
/// # 返回
/// 返回已授权业务用例的结果；失败保留既有错误类型。
pub async fn invoice_request_cancel(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelInvoiceRequest>,
) -> Result<InvoiceRequestView> {
    Ok(ApiResponse::ok_with_data(
        ReceivableProcess::new(state.application().db(), receivable_posting_ports(state.application().db()))
            .cancel_invoice_request(&id, req, &actor)
            .await?,
    ))
}
