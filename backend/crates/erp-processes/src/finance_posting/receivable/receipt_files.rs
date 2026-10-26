//! 回款凭证只接收本次上传，随草稿在同一事务建立附件关系。
use std::collections::HashSet;

use application_core::{AuditActor, CommandReceipt};
use erp_core::ids::{BusinessDocumentId, DocumentAttachmentId, FileAssetId};
use erp_finance::dto::receivable::SaveCustomerReceiptDraftRequest;
use erp_support::repository::prelude::*;
use erp_support::{
    AttachmentUsage, DocumentAttachment, DocumentAttachmentData, FileAssetExt, PendingAttachmentBatch,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 保留无附件旧命令指纹，上传内容摘要参与带附件命令身份。
pub(super) fn receipt(
    req: &SaveCustomerReceiptDraftRequest,
    actor: &AuditActor,
    pending: &dyn PendingAttachmentBatch,
) -> Result<CommandReceipt> {
    let manifest = pending.content_manifest();
    if manifest.is_empty() && !pending.is_empty() {
        return Err(Error::ValidationError("回款凭证缺少文件内容清单".into()));
    }
    if manifest.is_empty() {
        return Ok(CommandReceipt::from_payload(
            "customer-receipt-draft-",
            actor.id(),
            "customer_receipt.save_draft",
            "customer_receipt",
            &req.idempotency_key,
            req,
        )?);
    }
    Ok(CommandReceipt::from_payload(
        "customer-receipt-draft-",
        actor.id(),
        "customer_receipt.save_draft",
        "customer_receipt",
        &req.idempotency_key,
        &serde_json::json!({"request": req, "files": manifest}),
    )?)
}

/// 只解析本批临时文件，不允许将任意已有资产挂接到回款。
pub(super) fn resolve(ids: &mut [FileAssetId], pending: &dyn PendingAttachmentBatch) -> Result<()> {
    let mut used = HashSet::new();
    let mut unique = HashSet::new();
    for id in ids {
        if !id.as_ref().starts_with("pending-file:") {
            return Err(Error::ValidationError("回款凭证必须使用本次上传的文件".into()));
        }
        pending.resolve_id(id, &mut used)?;
        if !unique.insert(id.clone()) {
            return Err(Error::ValidationError("回款凭证不能重复".into()));
        }
    }
    pending.ensure_all_used(&used)?;
    Ok(())
}

/// 本单累计凭证不得超出上传上限。
fn check_count(existing: usize, added: usize) -> Result<()> {
    if existing + added > 32 {
        return Err(Error::ValidationError("每笔回款最多保存 32 个凭证文件".into()));
    }
    Ok(())
}

/// 与草稿共用执行器登记本批资产和附件，旧凭证保留。
pub(super) async fn persist(
    db: &Database,
    id: &str,
    ids: &[FileAssetId],
    pending: &dyn PendingAttachmentBatch,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let document_id = BusinessDocumentId::new(id);
    let existing = db.document_attachments().list_by_document(&document_id, executor).await?;
    check_count(existing.len(), ids.len())?;
    pending.persist(db, executor).await?;
    for file_id in ids {
        let attachment = DocumentAttachment::new(
            DocumentAttachmentId::new(id_generator::next_id()),
            DocumentAttachmentData {
                document_id: document_id.clone(),
                file_asset_id: file_id.clone(),
                usage: AttachmentUsage::Attachment,
                created_by: actor.id().into(),
            },
        )?;
        db.document_attachments().create(&attachment, executor).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_support::EmptyPendingAttachments;

    use super::*;
    #[test]
    fn receipt_files_reject_external_assets_missing_uploads_and_excess_count() {
        resolve(&mut [], &EmptyPendingAttachments).unwrap();
        assert!(resolve(&mut [FileAssetId::new("asset")], &EmptyPendingAttachments).is_err());
        assert!(resolve(&mut [FileAssetId::new("pending-file:missing")], &EmptyPendingAttachments).is_err());
        check_count(31, 1).unwrap();
        assert!(check_count(32, 1).is_err());
    }
    #[test]
    fn receipt_upload_replay_binds_content_and_preserves_legacy_payload() {
        use erp_core::AccountKind;
        use erp_support::{
            PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass, SensitivityClass,
        };

        use crate::attachments::PendingFileAssets;
        let actor = AuditActor::new("actor".into(), "财务".into(), AccountKind::Admin);
        let json = serde_json::json!({"receipt_id":null,"expected_version":null,"receipt":{"receipt_no":"R1","counterparty_party_id":"p","customer_id":null,"received_at":1,"amount":"10.00","bank_reference":null},"allocations":[],"idempotency_key":"same"});
        let mut req: SaveCustomerReceiptDraftRequest = serde_json::from_value(json).unwrap();
        let old = CommandReceipt::from_payload(
            "customer-receipt-draft-",
            actor.id(),
            "customer_receipt.save_draft",
            "customer_receipt",
            &req.idempotency_key,
            &req,
        )
        .unwrap();
        assert!(
            !serde_json::to_value(&req).unwrap().as_object().unwrap().contains_key("attachment_asset_ids")
        );
        assert_eq!(receipt(&req, &actor, &EmptyPendingAttachments).unwrap().fingerprint(), old.fingerprint());
        req.attachment_asset_ids = vec![FileAssetId::new("pending-file:receipt")];
        let batch = |digest: &str, key: &str| {
            PendingFileAssets::prepare(
                vec![PendingFileAssetRequest {
                    reference: "pending-file:receipt".into(),
                    registration: RegisterFileAssetRequest {
                        storage_object_key: key.into(),
                        file_name: "回款凭证.pdf".into(),
                        content_type: "application/pdf".into(),
                        byte_size: 12,
                        content_hmac: digest.repeat(64),
                        sensitivity_class: SensitivityClass::Sensitive,
                        retention_class: RetentionClass::LongTerm,
                        expires_at: None,
                    },
                }],
                &actor,
            )
            .unwrap()
        };
        let first = batch("a", "first");
        let retry = batch("a", "retry");
        let changed = batch("b", "changed");
        assert_eq!(
            receipt(&req, &actor, &first).unwrap().fingerprint(),
            receipt(&req, &actor, &retry).unwrap().fingerprint()
        );
        assert_ne!(
            receipt(&req, &actor, &first).unwrap().fingerprint(),
            receipt(&req, &actor, &changed).unwrap().fingerprint()
        );
        let mut ids = req.attachment_asset_ids.clone();
        resolve(&mut ids, &first).unwrap();
        assert!(!ids[0].as_ref().starts_with("pending-file:"));
        assert!(resolve(&mut [], &first).is_err());
        assert!(
            resolve(
                &mut [FileAssetId::new("pending-file:receipt"), FileAssetId::new("pending-file:receipt")],
                &first
            )
            .is_err()
        );
    }
}
