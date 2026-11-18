//! 域 D19 `payable` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_finance::dto::payable` 的 DTO。

use crate::wiring::finance_posting_ports;
pub(crate) mod invoice_files;

use application_core::AuditActor;
use axum::Extension;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use erp_finance::dto::payable::{
    CommitSupplierPaymentRequest, CreatePayableAccountRequest, PayableAccountListParams, PayableAccountView,
    PaymentRecipientRevealView, PurchaseInvoiceAllocationListParams, PurchaseInvoiceRegisteredView,
    RegisterPurchaseInvoiceRequest, RevealPaymentRecipientRequest, SupplierPaymentListParams,
    SupplierPaymentView,
};
use erp_finance::dto::payment_merge::{PaymentMergeCandidatesParams, PaymentMergeCandidatesView};
use erp_finance_posting::finance_posting::payable::PayableProcess;
use erp_read_models::finance::funds_scope::{
    FundsScopedPage, FundsScopedResult, ScopedPayableAccountRow, ScopedPurchaseInvoiceAllocationRow,
    ScopedSupplierPaymentRow,
};
use erp_support::SecurityScanStatus;
use tracing::error;

use crate::http::errors::{Error, Result};
use crate::http::extractor::{Json, Query};
use crate::http::handler::file_asset::PendingAssetFile;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::funds_access;

/// 按请求条件查询应付往来子账列表。
///
/// DataScope v2：按来源采购单当前采购负责人查询，同一事务快照；
/// 部分授权仅返获授权份额，整单金额为 null，跨页原样回传 `scope_version`。
/// 返回范围分页视图（`items`/`total`/`summary`/`scope_version`）。采购负责人候选不随列表返回。
pub async fn payable_account_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PayableAccountListParams>,
) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
    let page = funds_access(state.application().db(), state.application().authorization())
        .payable_account_list_scoped(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询应付往来子账详情（DataScope v2 范围裁剪行）。
/// 返回范围裁剪后的子账行；不可见与不存在统一为 NotFound。
pub async fn payable_account_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FundsScopedResult<ScopedPayableAccountRow>> {
    let view = funds_access(state.application().db(), state.application().authorization())
        .payable_account_detail_scoped(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 在付款任务责任、版本和收款账户身份校验后揭示完整收款账号。
/// `id` 为当前付款任务绑定的应付往来子账 ID。
/// 返回完整收款账号；成功揭示同时记录敏感数据审计。
pub async fn payment_recipient_reveal(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RevealPaymentRecipientRequest>,
) -> Result<PaymentRecipientRevealView> {
    let sensitive_data = state.application().sensitive_data();
    let view = PayableProcess::new(
        state.application().db(),
        finance_posting_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .reveal_payment_recipient(&id, req, &actor, sensitive_data.as_ref())
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 建立应付往来子账与原始应付分录（跨集合事务）。
/// 返回新建子账的响应视图。
pub async fn payable_account_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePayableAccountRequest>,
) -> Result<PayableAccountView> {
    let view = PayableProcess::new(
        state.application().db(),
        finance_posting_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .create_payable_account(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按请求条件查询供应商付款单列表。
///
/// DataScope v2：按核销关联采购当前负责人与付款经办人分别查询；
/// 付款金额按分配事实只算匹配份额，未分配单列。
/// 返回范围分页视图（`items`/`total`/`summary`/`scope_version`）。采购负责人候选不随列表返回。
pub async fn supplier_payment_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SupplierPaymentListParams>,
) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
    let page = funds_access(state.application().db(), state.application().authorization())
        .supplier_payment_list_scoped(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询供应商付款单详情（DataScope v2 范围裁剪行）。
/// 返回范围裁剪后的付款行；不可见与不存在统一为 NotFound。
pub async fn supplier_payment_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FundsScopedResult<ScopedSupplierPaymentRow>> {
    let view = funds_access(state.application().db(), state.application().authorization())
        .supplier_payment_detail_scoped(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 查询当前付款任务可合并的同供应商开放任务。
/// 返回当前任务及同一供应商下可勾选的其它开放付款任务。
pub async fn supplier_payment_merge_candidates(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PaymentMergeCandidatesParams>,
) -> Result<PaymentMergeCandidatesView> {
    let view = PayableProcess::new(
        state.application().db(),
        finance_posting_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .payment_merge_candidates(params, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 拒绝已停止的采购应付直付入口。
/// 无成功结果；付款须从已批准的付款申请登记。
/// 始终返回已停止入口的业务错误，不读取文件或写入存储。
pub async fn supplier_payment_commit(
    State(_state): State<HttpState>,
    Extension(_actor): Extension<AuditActor>,
) -> Result<SupplierPaymentView> {
    Ok(ApiResponse::ok_with_data(PayableProcess::reject_client_payment()?))
}

/// 在校验付款归属并记录审计后，返回银行回单图片内容。
/// 付款、回单或对象不存在，文件不可预览，审计或对象存储读取失败时返回错误。
pub async fn supplier_payment_bank_receipt(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let payable = PayableProcess::new(
        state.application().db(),
        finance_posting_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    );
    let snapshot = payable.supplier_payment_bank_receipt(&id, &actor).await?;
    let view = &snapshot.asset;
    if !matches!(view.content_type.as_str(), "image/jpeg" | "image/png" | "image/webp") {
        return Err(Error::Unprocessable("当前银行回单类型不支持在线预览".to_string()));
    }
    if view.destroyed_at.is_some()
        || matches!(view.security_scan_status, SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined)
    {
        return Err(Error::Unprocessable("银行回单不可预览，请联系管理员".to_string()));
    }
    let content = state
        .application()
        .storage()
        .read_immutable(&view.storage_object_key, &view.content_hmac)
        .await
        .map_err(|storage_error| {
            error!(
                error = %storage_error,
                supplier_payment_id = %id,
                "Failed to read supplier payment bank receipt"
            );
            Error::Internal("Object storage operation failed".to_string())
        })?;
    payable.revalidate_supplier_payment_bank_receipt(&id, &actor, &snapshot).await?;
    let content_type = HeaderValue::from_str(&view.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let mut response = Response::new(Body::from(content));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(CONTENT_TYPE, content_type);
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

// 入库前核对 multipart 里的银行回单字段与付款命令中的临时引用，避免引用缺失或多传文件。
pub(crate) fn validate_bank_receipt_upload(
    req: &CommitSupplierPaymentRequest,
    files: &[PendingAssetFile],
) -> std::result::Result<(), Error> {
    let reference = req.payment.bank_receipt_asset_id.to_string();
    let file = match (reference.starts_with("pending-file:"), files) {
        (false, []) => return Ok(()),
        (true, [file]) => file,
        _ => return Err(Error::BadRequest("银行回单图片与付款命令不匹配".to_string())),
    };
    if file.reference != reference {
        return Err(Error::BadRequest("银行回单图片临时引用无效".to_string()));
    }
    if !matches!(file.file.content_type.as_str(), "image/jpeg" | "image/png" | "image/webp") {
        return Err(Error::BadRequest("银行回单仅支持 JPG、PNG 或 WebP 图片".to_string()));
    }
    Ok(())
}

/// 进项发票登记过账并分配（§8.3-2 事务不变量，资金入口幂等去重）。
///
/// 发票实体经 D18 `invoices()` 仓储写入，D19 只写进项发票分配。
/// 返回登记后发票与分配行视图。
pub async fn purchase_invoice_allocation_post(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterPurchaseInvoiceRequest>,
) -> Result<PurchaseInvoiceRegisteredView> {
    let view = PayableProcess::new(
        state.application().db(),
        finance_posting_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .register_purchase_invoice(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 查询进项发票分配列表（按应付子账筛选）。
///
/// DataScope v2：按应付子账来源采购当前负责人与收票经办人分别查询；
/// 部分授权仅返获授权份额，整单分配金额为 null。
/// `query` 为分页与筛选参数（`payable_account_id` 必填）。
/// 返回范围分页视图（`items`/`total`/`summary`/`scope_version`）。采购负责人候选不随列表返回。
pub async fn purchase_invoice_allocation_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PurchaseInvoiceAllocationListParams>,
) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
    let page = funds_access(state.application().db(), state.application().authorization())
        .purchase_invoice_allocation_list_scoped(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[cfg(test)]
mod tests {
    use erp_finance::dto::payable::CommitSupplierPaymentRequest;

    use super::{Error, validate_bank_receipt_upload};
    use crate::http::handler::file_asset::{AssetFile, PendingAssetFile};

    fn payment_request(asset_id: &str) -> CommitSupplierPaymentRequest {
        serde_json::from_value(serde_json::json!({
            "payment_request_id": "ppr-1",
            "expected_request_version": 1,
            "work_item_id": "wi-1",
            "expected_task_version": "1",
            "expected_payee_bank_account_id": "bank-1",
            "expected_payee_bank_account_version": 1,
            "payment": {
                "payment_no": "FK-1",
                "supplier_id": "supplier-1",
                "paid_at": 1,
                "amount": "10.00",
                "bank_reference": null,
                "bank_receipt_asset_id": asset_id
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10.00"}],
            "idempotency_key": "commit-1"
        }))
        .expect("付款命令必须可反序列化")
    }

    fn pending_file(reference: &str, content_type: &str) -> PendingAssetFile {
        PendingAssetFile {
            reference: reference.into(),
            file: AssetFile {
                file_name: "receipt.png".to_string(),
                content_type: content_type.into(),
                content: vec![1],
            },
        }
    }

    #[test]
    fn bank_receipt_file_must_match_pending_image_reference() {
        let request = payment_request("pending-file:bank-receipt");
        for content_type in ["image/jpeg", "image/png", "image/webp"] {
            let image = pending_file("pending-file:bank-receipt", content_type);
            assert!(validate_bank_receipt_upload(&request, &[image]).is_ok());
        }
        let pdf = pending_file("pending-file:bank-receipt", "application/pdf");
        assert!(matches!(
            validate_bank_receipt_upload(&request, &[pdf]),
            Err(Error::BadRequest(message)) if message == "银行回单仅支持 JPG、PNG 或 WebP 图片"
        ));
        assert!(validate_bank_receipt_upload(&request, &[]).is_err());
    }

    #[test]
    fn bank_receipt_count_is_checked_before_reference_and_type() {
        assert!(validate_bank_receipt_upload(&payment_request("asset-1"), &[]).is_ok());
        for (asset_id, files) in [
            ("pending-file:bank-receipt", vec![]),
            ("asset-1", vec![pending_file("wrong", "application/pdf")]),
            (
                "pending-file:bank-receipt",
                vec![pending_file("wrong-1", "application/pdf"), pending_file("wrong-2", "application/pdf")],
            ),
        ] {
            assert!(matches!(
                validate_bank_receipt_upload(&payment_request(asset_id), &files),
                Err(Error::BadRequest(message)) if message == "银行回单图片与付款命令不匹配"
            ));
        }
    }

    #[test]
    fn bank_receipt_reference_is_checked_before_content_type() {
        let request = payment_request("pending-file:bank-receipt");
        let file = pending_file("pending-file:other", "application/pdf");
        assert!(matches!(
            validate_bank_receipt_upload(&request, &[file]),
            Err(Error::BadRequest(message)) if message == "银行回单图片临时引用无效"
        ));
    }
}
