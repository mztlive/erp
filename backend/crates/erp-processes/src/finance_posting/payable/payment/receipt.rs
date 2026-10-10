//! 新付款仅消费本命令上传的回单；已提交重放在进入准备阶段前查证原回执。

use std::collections::HashSet;

use erp_core::ids::FileAssetId;
use erp_support::{BankReceiptEvidencePolicy, FileAssetExt, PendingAttachmentBatch};
use mongodb::Database;
use persistence_core::Executor;

use super::CommitSupplierPaymentRequest;
use crate::{Error, Result};

/// 解析并恰好消费本批次回单，拒绝把既有正式资产挂入新的付款关系。
pub(super) fn resolve_payment_receipt(
    req: &mut CommitSupplierPaymentRequest,
    pending_assets: &dyn PendingAttachmentBatch,
) -> Result<()> {
    let mut used = HashSet::new();
    let resolved = pending_assets.resolve_id(&mut req.payment.bank_receipt_asset_id, &mut used)?;
    if !resolved || !pending_assets.contains_id(&req.payment.bank_receipt_asset_id) {
        return Err(Error::ValidationError("新付款必须上传本次银行回单，不得引用既有文件资产".into()));
    }
    pending_assets.ensure_all_used(&used)?;
    Ok(())
}

/// 在同一事务内确认新持久化回单属于本批次，并执行领域证据政策。
pub(super) async fn ensure_bank_receipt_asset(
    db: &Database,
    asset_id: &FileAssetId,
    pending_assets: &dyn PendingAttachmentBatch,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !pending_assets.contains_id(asset_id) {
        return Err(Error::ValidationError("银行回单不属于本次上传批次".into()));
    }
    let asset = db
        .file_assets()
        .find_by_id(asset_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("本次银行回单尚未登记".into()))?;
    BankReceiptEvidencePolicy::validate(
        &asset.content_type,
        asset.sensitivity_class,
        asset.retention_class,
        asset.destroyed_at.is_some(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use application_core::{AuditActor, CommandReceipt};
    use erp_core::AccountKind;
    use erp_core::ids::FileAssetId;
    use erp_finance::FinanceCommandReceipt;
    use erp_support::{
        EmptyPendingAttachments, PendingAttachmentBatch, PendingFileAssetRequest, RegisterFileAssetRequest,
        RetentionClass, SensitivityClass,
    };

    use super::{CommitSupplierPaymentRequest, Error, resolve_payment_receipt};
    use crate::attachments::PendingFileAssets;

    fn command(reference: &str) -> CommitSupplierPaymentRequest {
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
                "bank_receipt_asset_id": reference
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10.00"}],
            "idempotency_key": "commit-1"
        }))
        .unwrap()
    }

    fn batch(references: &[&str]) -> PendingFileAssets {
        let actor = AuditActor::new("cashier-1".into(), "fukuan".into(), AccountKind::Admin);
        let requests = references
            .iter()
            .map(|reference| PendingFileAssetRequest {
                reference: (*reference).into(),
                registration: RegisterFileAssetRequest {
                    storage_object_key: format!("uploads/{reference}"),
                    file_name: "receipt.png".into(),
                    content_type: "image/png".into(),
                    byte_size: 12,
                    content_hmac: "a".repeat(64),
                    sensitivity_class: SensitivityClass::Sensitive,
                    retention_class: RetentionClass::LongTerm,
                    expires_at: None,
                },
            })
            .collect();
        PendingFileAssets::prepare(requests, &actor).unwrap()
    }

    fn command_receipt(req: &CommitSupplierPaymentRequest) -> CommandReceipt {
        CommandReceipt::from_payload(
            "supplier-payment-commit-",
            "cashier-1",
            "supplier_payment.commit",
            "supplier_payment",
            &req.idempotency_key,
            req,
        )
        .unwrap()
    }

    #[test]
    fn new_payment_consumes_only_its_uploaded_receipt() {
        let pending = batch(&["pending-file:receipt"]);
        let mut req = command("pending-file:receipt");
        resolve_payment_receipt(&mut req, &pending).unwrap();
        assert!(pending.contains_id(&req.payment.bank_receipt_asset_id));
        assert!(!req.payment.bank_receipt_asset_id.as_ref().starts_with("pending-file:"));
        assert_eq!(req.allocations[0].payable_entry_id.as_ref(), "pe-1");
        assert_eq!(req.idempotency_key, "commit-1");
    }

    #[test]
    fn new_payment_rejects_existing_receipt_without_upload() {
        let mut req = command("another-payment-receipt");
        let error = resolve_payment_receipt(&mut req, &EmptyPendingAttachments).unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message.contains("不得引用既有")));
        assert_eq!(req.payment.bank_receipt_asset_id.as_ref(), "another-payment-receipt");
    }

    #[test]
    fn new_payment_rejects_existing_receipt_even_with_an_unused_upload() {
        let pending = batch(&["pending-file:receipt"]);
        let mut req = command("another-payment-receipt");
        let error = resolve_payment_receipt(&mut req, &pending).unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message.contains("不得引用既有")));
    }

    #[test]
    fn new_payment_rejects_unuploaded_temporary_receipt() {
        let pending = batch(&["pending-file:receipt"]);
        let mut req = command("pending-file:missing");
        let error = resolve_payment_receipt(&mut req, &pending).unwrap_err();
        assert!(error.to_string().contains("业务命令引用了未上传的文件"));
    }

    #[test]
    fn new_payment_without_upload_cannot_reuse_a_temporary_reference() {
        let mut req = command("pending-file:receipt");
        let error = resolve_payment_receipt(&mut req, &EmptyPendingAttachments).unwrap_err();
        assert!(error.to_string().contains("业务命令引用了未上传的文件"));
    }

    #[test]
    fn new_payment_rejects_extra_unconsumed_upload() {
        let pending = batch(&["pending-file:receipt", "pending-file:extra"]);
        let mut req = command("pending-file:receipt");
        let error = resolve_payment_receipt(&mut req, &pending).unwrap_err();
        assert!(error.to_string().contains("存在未被业务命令引用的上传文件"));
    }

    #[test]
    fn freshly_allocated_formal_id_does_not_bypass_temporary_reference_requirement() {
        let pending = batch(&["pending-file:receipt"]);
        let mut req = command("pending-file:receipt");
        resolve_payment_receipt(&mut req, &pending).unwrap();
        let id = FileAssetId::new(req.payment.bank_receipt_asset_id.to_string());
        assert!(pending.contains_id(&id));
        let error = resolve_payment_receipt(&mut req, &pending).unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message.contains("不得引用既有")));
    }

    #[test]
    fn original_payload_replays_receipt_without_preparing_another_upload() {
        let req = command("pending-file:receipt");
        let original = command_receipt(&req);
        let receipt =
            FinanceCommandReceipt::resource(&original, "payment-1".into(), "event-1".into()).unwrap();
        let pending = batch(&["pending-file:receipt"]);
        let mut prepared_req = req.clone();
        resolve_payment_receipt(&mut prepared_req, &pending).unwrap();
        assert_ne!(prepared_req.payment.bank_receipt_asset_id, req.payment.bank_receipt_asset_id);

        let retried = command_receipt(&req);
        assert_eq!(receipt.resource_id(&retried).unwrap(), "payment-1");
        assert_eq!(req.payment.bank_receipt_asset_id.as_ref(), "pending-file:receipt");
        // 首笔准备后的正式身份不属于原命令载荷；恢复必须沿用原始临时引用。
        assert!(receipt.resource_id(&command_receipt(&prepared_req)).is_err());
    }

    #[test]
    fn same_key_with_foreign_receipt_cannot_replay_original_payment() {
        let original_req = command("pending-file:receipt");
        let receipt = FinanceCommandReceipt::resource(
            &command_receipt(&original_req),
            "payment-1".into(),
            "event-1".into(),
        )
        .unwrap();
        let foreign_req = command("another-payment-receipt");
        assert!(receipt.resource_id(&command_receipt(&foreign_req)).is_err());
    }
}
