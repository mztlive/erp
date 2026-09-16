//! 域 D19 `payable` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_finance::dto::payable` 的 DTO。

use application_core::AuditActor;
use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use axum::{Extension, Json};
use erp_finance::dto::payable::{
    CommitSupplierPaymentRequest, CreatePayableAccountRequest, PayableAccountListParams, PayableAccountView,
    PaymentRecipientRevealView, PurchaseInvoiceAllocationListParams, PurchaseInvoiceRegisteredView,
    RegisterPurchaseInvoiceRequest, RevealPaymentRecipientRequest, SupplierPaymentListParams,
    SupplierPaymentView,
};
use erp_finance::dto::payment_merge::{PaymentMergeCandidatesParams, PaymentMergeCandidatesView};
use erp_processes::finance_posting::payable::PayableService;
use erp_read_models::finance::funds_scope::{
    FundsScopedPage, FundsScopedResult, ScopedPayableAccountRow, ScopedPurchaseInvoiceAllocationRow,
    ScopedSupplierPaymentRow,
};
use erp_support::{SecurityScanStatus, SensitivityClass};
use tracing::error;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::{
    PendingAssetFile, delete_pending_asset_objects, extract_command_with_asset_files,
    should_compensate_pending_assets, store_pending_asset_files,
};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "查询应付往来子账列表",
    resource = "payable_account",
    action = "list"
)]
/// 查询应付往来子账列表。
///
/// DataScope v2：按来源采购单当前采购负责人查询，同一事务快照；
/// 部分授权仅返获授权份额，整单金额为 null，跨页原样回传 `scope_version`。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的当前操作人
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回范围分页视图（`items`/`total`/`summary`/`owner_options`/`scope_version`）。
pub async fn payable_account_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PayableAccountListParams>,
) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
    let purchase_access = erp_processes::adapters::purchase_access(state.db(), state.rbac());
    let page = erp_processes::adapters::funds_access_with_rbac(state.db(), state.rbac())
        .payable_account_list_scoped(&params, &actor, &purchase_access)
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
/// 查询应付往来子账详情（DataScope v2 范围裁剪行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的当前操作人
/// * `id` - 应付往来子账 ID
///
/// # 返回
/// 返回范围裁剪后的子账行；不可见与不存在统一为 NotFound。
pub async fn payable_account_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FundsScopedResult<ScopedPayableAccountRow>> {
    let purchase_access = erp_processes::adapters::purchase_access(state.db(), state.rbac());
    let view = erp_processes::adapters::funds_access_with_rbac(state.db(), state.rbac())
        .payable_account_detail_scoped(&id, &actor, &purchase_access)
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
        .with_object_read(state.approval_object_read())
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
        .with_object_read(state.approval_object_read())
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
/// DataScope v2：按核销关联采购当前负责人与付款经办人分别查询；
/// 付款金额按分配事实只算匹配份额，未分配单列。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的当前操作人
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回范围分页视图（`items`/`total`/`summary`/`owner_options`/`scope_version`）。
pub async fn supplier_payment_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SupplierPaymentListParams>,
) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
    let purchase_access = erp_processes::adapters::purchase_access(state.db(), state.rbac());
    let page = erp_processes::adapters::funds_access_with_rbac(state.db(), state.rbac())
        .supplier_payment_list_scoped(&params, &actor, &purchase_access)
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
/// 查询供应商付款单详情（DataScope v2 范围裁剪行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的当前操作人
/// * `id` - 付款单 ID
///
/// # 返回
/// 返回范围裁剪后的付款行；不可见与不存在统一为 NotFound。
pub async fn supplier_payment_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FundsScopedResult<ScopedSupplierPaymentRow>> {
    let purchase_access = erp_processes::adapters::purchase_access(state.db(), state.rbac());
    let view = erp_processes::adapters::funds_access_with_rbac(state.db(), state.rbac())
        .supplier_payment_detail_scoped(&id, &actor, &purchase_access)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "查询可合并的同供应商付款任务",
    resource = "payable_account",
    action = "detail"
)]
/// 查询当前付款任务可合并的同供应商开放任务。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `params` - 当前工作台打开的付款执行任务
///
/// # 返回
/// 返回当前任务及同一供应商下可勾选的其它开放付款任务。
pub async fn supplier_payment_merge_candidates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PaymentMergeCandidatesParams>,
) -> Result<PaymentMergeCandidatesView> {
    let view = PayableService::new(state.db())
        .with_object_read(state.approval_object_read())
        .payment_merge_candidates(params, &actor)
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
    let result = erp_processes::commit_supplier_payment_with_assets(
        state.db(),
        state.approval_object_read(),
        req,
        pending.clone(),
        actor,
    )
    .await;
    match result {
        Ok(result) => {
            if !result.assets_committed {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Ok(ApiResponse::ok_with_data(result.view))
        },
        Err(service_error) => {
            if should_compensate_pending_assets(&service_error) {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Err(service_error.into())
        },
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
        .with_object_read(state.approval_object_read())
        .supplier_payment_bank_receipt(&id, &actor)
        .await?;
    if !matches!(view.content_type.as_str(), "image/jpeg" | "image/png" | "image/webp") {
        return Err(Error::Unprocessable("当前银行回单类型不支持在线预览".to_string()));
    }
    if view.destroyed_at.is_some()
        || matches!(view.security_scan_status, SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined)
    {
        return Err(Error::Unprocessable("银行回单不可预览，请联系管理员".to_string()));
    }
    let content = state.storage().read(&view.storage_object_key).await.map_err(|storage_error| {
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
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
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
    let mut actual = files.iter().map(|pending| pending.reference.clone()).collect::<Vec<_>>();
    actual.sort();
    if actual != expected {
        return Err(Error::BadRequest("银行回单图片临时引用无效".to_string()));
    }
    if files.iter().any(|pending| {
        !matches!(pending.file.content_type.as_str(), "image/jpeg" | "image/png" | "image/webp")
    }) {
        return Err(Error::BadRequest("银行回单仅支持 JPG、PNG 或 WebP 图片".to_string()));
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
        .with_object_read(state.approval_object_read())
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
/// DataScope v2：按应付子账来源采购当前负责人与收票经办人分别查询；
/// 部分授权仅返获授权份额，整单分配金额为 null。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的当前操作人
/// * `query` - 分页与筛选参数（`payable_account_id` 必填）
///
/// # 返回
/// 返回范围分页视图（`items`/`total`/`summary`/`owner_options`/`scope_version`）。
pub async fn purchase_invoice_allocation_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PurchaseInvoiceAllocationListParams>,
) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
    let purchase_access = erp_processes::adapters::purchase_access(state.db(), state.rbac());
    let page = erp_processes::adapters::funds_access_with_rbac(state.db(), state.rbac())
        .purchase_invoice_allocation_list_scoped(&params, &actor, &purchase_access)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[cfg(test)]
mod tests {
    use erp_finance::dto::payable::CommitSupplierPaymentRequest;

    use super::validate_bank_receipt_upload;
    use crate::core::handler::file_asset::{AssetFile, PendingAssetFile};

    /// 供应商付款 HTTP 只保留任务内原子登记与详情，不暴露付款审批端口。
    #[test]
    fn supplier_payment_http_uses_execution_task_port() {
        let production = include_str!("mod.rs").split("#[cfg(test)]").next().expect("生产代码");
        assert!(production.contains("commit_supplier_payment_with_assets"));
        assert!(production.contains("reveal_payment_recipient"));
        assert!(production.contains("supplier_payment_detail"));
        assert!(production.contains("payment_merge_candidates"));
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
