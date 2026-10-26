//! 回款草稿文件与登记命令共同提交。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, State};
use erp_finance::dto::receivable::SaveCustomerReceiptDraftRequest;
use erp_processes::attachments::PendingFileAssets;
use erp_processes::finance_posting::receivable::ReceivableProcess;
use erp_read_models::finance::dto::CustomerReceiptView;
use erp_support::SensitivityClass;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::handler::file_asset::{
    delete_pending_asset_objects, extract_command_with_asset_files, finish_asset_command,
    store_pending_asset_files,
};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款与销项发票管理",
    desc = "保存回款草稿及凭证附件",
    resource = "customer_receipt",
    action = "submit"
)]
/// 在登记回款草稿时上传文件，复用原登记权限与事务。
/// # 参数
/// `state` 为应用状态，`actor` 为认证账号，`multipart` 为命令及其引用文件。
/// # 返回
/// 原回款草稿登记视图。
/// # 错误
/// 文件、业务或事务失败时拒绝；确定未消费的上传删除，未知提交保留。
pub async fn customer_receipt_save_with_files(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<CustomerReceiptView> {
    let (req, files) =
        extract_command_with_asset_files::<SaveCustomerReceiptDraftRequest>(&mut multipart).await?;
    let uploads = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let pending = match PendingFileAssets::prepare(uploads.clone(), &actor) {
        Ok(pending) => pending.shared(),
        Err(error) => {
            delete_pending_asset_objects(&state, &uploads).await;
            return Err(error.into());
        },
    };
    let outcome = ReceivableProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .save_customer_receipt_draft_with_assets(req, pending, &actor)
        .await;
    let result = finish_asset_command(&state, &uploads, outcome, |result| result.assets_committed).await?;
    Ok(ApiResponse::ok_with_data(result.view))
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

    /// 通过真实 multipart 提取器构造回款命令与文件请求。
    async fn extract(
        file_content: &str,
    ) -> std::result::Result<(SaveCustomerReceiptDraftRequest, Vec<PendingAssetFile>), Error> {
        let command = json!({"receipt_id": null, "expected_version": null, "receipt": {"receipt_no":"R1","counterparty_party_id":"p","received_at":1,"amount":"10.00"}, "allocations": [], "idempotency_key":"receipt-file", "attachment_asset_ids":["pending-file:invoice"]});
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
    async fn receipt_draft_accepts_pdf_before_command_and_rejects_spoofed_content() {
        let (req, files) = extract("%PDF-1.7\ninvoice").await.unwrap();
        assert_eq!(req.attachment_asset_ids[0].as_ref(), "pending-file:invoice");
        assert_eq!(files.len(), 1);
        assert!(extract("not a pdf").await.is_err());
    }
}
