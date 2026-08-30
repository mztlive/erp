//! 域 D19 `payable` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::payable` 的 DTO。

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS},
        HeaderValue, StatusCode,
    },
    response::Response,
    Extension, Json,
};
use entities::file_asset::{SecurityScanStatus, SensitivityClass};
use services::{
    audit::AuditActor,
    payable::{
        CommitSupplierPaymentRequest, CreatePayableAccountRequest, PageView, PayableAccountListParams,
        PayableAccountSummaryView, PayableAccountView, PayableService, PaymentRecipientRevealView,
        PurchaseInvoiceAllocationListParams, PurchaseInvoiceAllocationView, PurchaseInvoiceRegisteredView,
        RegisterPurchaseInvoiceRequest, RevealPaymentRecipientRequest, SupplierPaymentListParams,
        SupplierPaymentView,
    },
};
use tracing::error;

use crate::{
    app_state::AppState,
    core::{
        errors::{Error, Result},
        handler::file_asset::{
            delete_pending_asset_objects, extract_command_with_asset_files, should_compensate_pending_assets,
            store_pending_asset_files, PendingAssetFile,
        },
        response::ApiResponse,
    },
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
) -> Result<PageView<PayableAccountSummaryView>> {
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
    desc = "揭示付款任务收款账号",
    resource = "party_bank_account",
    action = "reveal"
)]
/// 在付款任务责任、版本和收款账户身份校验后揭示完整收款账号。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 当前付款任务绑定的应付往来子账 ID
/// * `req` - 任务版本与页面所见收款账户身份
///
/// # 返回
/// 返回完整收款账号；成功揭示同时记录敏感数据审计。
pub async fn payment_recipient_reveal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<RevealPaymentRecipientRequest>,
) -> Result<PaymentRecipientRevealView> {
    let sensitive_data = state.sensitive_data();
    let view = PayableService::new(state.db())
        .reveal_payment_recipient(&id, req, &actor, sensitive_data.as_ref())
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
    desc = "登记供应商付款并过账核销",
    resource = "supplier_payment",
    action = "commit"
)]
/// 在付款执行任务内原子登记付款、过账并核销。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 本次付款事实、冻结分配与幂等键
///
/// # 返回
/// 返回已过账的付款单视图。
pub async fn supplier_payment_commit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<SupplierPaymentView> {
    let (req, files) =
        extract_command_with_asset_files::<CommitSupplierPaymentRequest>(&mut multipart).await?;
    validate_bank_receipt_upload(&req, &files)?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let result = PayableService::new(state.db())
        .commit_supplier_payment_with_assets(req, pending.clone(), &actor)
        .await;
    match result {
        Ok(result) => {
            if !result.assets_committed {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Ok(ApiResponse::ok_with_data(result.view))
        }
        Err(service_error) => {
            if should_compensate_pending_assets(&service_error) {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Err(service_error.into())
        }
    }
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "预览供应商付款银行回单",
    resource = "supplier_payment",
    action = "detail"
)]
/// 在校验付款归属并记录审计后，返回银行回单图片内容。
///
/// # 错误
/// 付款、回单或对象不存在，文件不可预览，审计或对象存储读取失败时返回错误。
pub async fn supplier_payment_bank_receipt(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let view = PayableService::new(state.db())
        .supplier_payment_bank_receipt(&id, &actor)
        .await?;
    if !matches!(
        view.content_type.as_str(),
        "image/jpeg" | "image/png" | "image/webp"
    ) {
        return Err(Error::Unprocessable("当前银行回单类型不支持在线预览".to_string()));
    }
    if view.destroyed_at.is_some()
        || matches!(
            view.security_scan_status,
            SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined
        )
    {
        return Err(Error::Unprocessable("银行回单不可预览，请联系管理员".to_string()));
    }
    let content = state
        .storage()
        .read(&view.storage_object_key)
        .await
        .map_err(|storage_error| {
            error!(
                error = %storage_error,
                supplier_payment_id = %id,
                "Failed to read supplier payment bank receipt"
            );
            Error::Internal("Object storage operation failed".to_string())
        })?;
    let content_type = HeaderValue::from_str(&view.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let mut response = Response::new(Body::from(content));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(CONTENT_TYPE, content_type);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
        .headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

/// 校验 multipart 中的银行回单字段与命令临时引用一一对应。
fn validate_bank_receipt_upload(
    req: &CommitSupplierPaymentRequest,
    files: &[PendingAssetFile],
) -> std::result::Result<(), Error> {
    let mut expected = Vec::new();
    let reference = req.payment.bank_receipt_asset_id.to_string();
    if reference.starts_with("pending-file:") {
        expected.push(reference);
    }
    if expected.len() != files.len() {
        return Err(Error::BadRequest("银行回单图片与付款命令不匹配".to_string()));
    }
    expected.sort();
    let mut actual = files
        .iter()
        .map(|pending| pending.reference.clone())
        .collect::<Vec<_>>();
    actual.sort();
    if actual != expected {
        return Err(Error::BadRequest("银行回单图片临时引用无效".to_string()));
    }
    if files.iter().any(|pending| {
        !matches!(
            pending.file.content_type.as_str(),
            "image/jpeg" | "image/png" | "image/webp"
        )
    }) {
        return Err(Error::BadRequest(
            "银行回单仅支持 JPG、PNG 或 WebP 图片".to_string(),
        ));
    }
    Ok(())
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
    use services::payable::CommitSupplierPaymentRequest;

    use super::validate_bank_receipt_upload;
    use crate::core::handler::file_asset::{AssetFile, PendingAssetFile};

    /// 供应商付款 HTTP 只保留任务内原子登记与详情，不暴露付款审批端口。
    #[test]
    fn supplier_payment_http_uses_execution_task_port() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("commit_supplier_payment_with_assets"));
        assert!(production.contains("reveal_payment_recipient"));
        assert!(production.contains("supplier_payment_detail"));
        assert!(!production.contains("submit_supplier_payment"));
        assert!(!production.contains("cancel_supplier_payment_approval"));
        assert!(!production.contains("reject_client_post"));
        assert!(!production.contains(".post_supplier_payment("));
        assert!(!production.contains("definition_id"));
        assert!(!production.contains("PENDING_REVIEW"));
    }

    #[test]
    fn bank_receipt_file_must_match_pending_image_reference() {
        let request = serde_json::from_value::<CommitSupplierPaymentRequest>(serde_json::json!({
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
                "bank_receipt_asset_id": "pending-file:bank-receipt"
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10.00"}],
            "idempotency_key": "commit-1"
        }))
        .expect("付款命令必须可反序列化");
        let image = PendingAssetFile {
            reference: "pending-file:bank-receipt".to_string(),
            file: AssetFile {
                file_name: "receipt.png".to_string(),
                content_type: "image/png".to_string(),
                content: vec![1],
            },
        };
        assert!(validate_bank_receipt_upload(&request, &[image]).is_ok());

        let pdf = PendingAssetFile {
            reference: "pending-file:bank-receipt".to_string(),
            file: AssetFile {
                file_name: "receipt.pdf".to_string(),
                content_type: "application/pdf".to_string(),
                content: vec![1],
            },
        };
        assert!(validate_bank_receipt_upload(&request, &[pdf]).is_err());
        assert!(validate_bank_receipt_upload(&request, &[]).is_err());
    }
}
