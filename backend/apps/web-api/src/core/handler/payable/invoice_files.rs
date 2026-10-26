//! 进项发票文件与登记命令共同提交。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, State};
use erp_finance::dto::payable::{PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest};
use erp_processes::attachments::PendingFileAssets;
use erp_processes::finance_posting::payable::PayableProcess;
use erp_support::SensitivityClass;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::handler::file_asset::{
    delete_pending_asset_objects, extract_command_with_asset_files, finish_asset_command,
    store_pending_asset_files,
};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "供应商往来",
    group_desc = "应付台账、付款单与进项发票登记管理（W12）",
    desc = "上传发票文件并登记过账分配",
    resource = "purchase_invoice_allocation",
    action = "post"
)]
/// 在登记进项发票时上传文件，复用原登记权限与事务。
/// # 参数
/// `state` 为应用状态，`actor` 为认证账号，`multipart` 为命令及其引用文件。
/// # 返回
/// 原进项发票登记视图。
/// # 错误
/// 文件、业务或事务失败时拒绝；确定未消费的上传删除，未知提交保留。
pub async fn purchase_invoice_allocation_post_with_files(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<PurchaseInvoiceRegisteredView> {
    let (req, files) =
        extract_command_with_asset_files::<RegisterPurchaseInvoiceRequest>(&mut multipart).await?;
    let uploads = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let pending = match PendingFileAssets::prepare(uploads.clone(), &actor) {
        Ok(pending) => pending.shared(),
        Err(error) => {
            delete_pending_asset_objects(&state, &uploads).await;
            return Err(error.into());
        },
    };
    let outcome = PayableProcess::new(state.db())
        .with_rbac(state.rbac())
        .with_object_read(state.approval_object_read())
        .register_purchase_invoice_with_assets(req, pending, &actor)
        .await;
    let result = finish_asset_command(&state, &uploads, outcome, |result| result.assets_committed).await?;
    Ok(ApiResponse::ok_with_data(result.view?))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::Request;
    use serde_json::json;

    use super::*;
    use crate::core::errors::Error;
    use crate::core::handler::file_asset::PendingAssetFile;

    /// 通过真实 multipart 提取器构造票面命令与文件请求。
    async fn extract(
        file_content: &str,
    ) -> std::result::Result<(RegisterPurchaseInvoiceRequest, Vec<PendingAssetFile>), Error> {
        let command = json!({
            "idempotency_key": "purchase-invoice-file-test",
            "invoice_no": "12345678", "invoice_date": "2026-10-26",
            "gross_amount": "113.00", "net_amount": "100.00", "tax_amount": "13.00",
            "supplier_id": "supplier-1",
            "attachment_asset_ids": ["pending-file:invoice"],
            "allocations": [{"payable_account_id": "payable-1", "allocated_gross_amount": "113.00", "allocated_net_amount": "100.00", "allocated_tax_amount": "13.00"}]
        });
        let body = format!(
            "--x\r\nContent-Disposition: form-data; name=\"pending-file:invoice\"; filename=\"invoice.pdf\"\r\nContent-Type: application/pdf\r\n\r\n{file_content}\r\n--x\r\nContent-Disposition: form-data; name=\"command\"\r\n\r\n{command}\r\n--x--\r\n"
        );
        let request = Request::builder()
            .header("content-type", "multipart/form-data; boundary=x")
            .body(Body::from(body))
            .unwrap();
        let mut multipart = Multipart::from_request(request, &()).await.unwrap();
        extract_command_with_asset_files(&mut multipart).await
    }

    #[tokio::test]
    async fn purchase_invoice_accepts_pdf_before_command_and_rejects_spoofed_content() {
        let (req, files) = extract("%PDF-1.7\ninvoice").await.unwrap();
        assert_eq!(req.attachment_asset_ids[0].as_ref(), "pending-file:invoice");
        assert_eq!(files.len(), 1);
        assert!(extract("not a pdf").await.is_err());
    }
}
