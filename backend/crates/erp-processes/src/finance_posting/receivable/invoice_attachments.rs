//! 发票文件与发票事实共享事务，文件字节由 HTTP 边界提前写入。

use std::collections::HashSet;

use application_core::{AuditActor, CommandReceipt};
use erp_core::ids::{BusinessDocumentId, DocumentAttachmentId, FileAssetId};
use erp_finance::dto::receivable::CommitInvoiceRequest;
use erp_support::{
    AttachmentUsage, DocumentAttachment, DocumentAttachmentData, FileAssetExt, PendingAttachmentBatch,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 解析临时引用并拒绝重复或未消费的上传对象。
pub(super) fn resolve(ids: &mut [FileAssetId], pending: &dyn PendingAttachmentBatch) -> Result<()> {
    if ids.len() > 32 {
        return Err(Error::ValidationError("发票附件最多 32 个".into()));
    }
    let mut used = HashSet::new();
    let mut unique = HashSet::new();
    for id in ids {
        pending.resolve_id(id, &mut used)?;
        if !unique.insert(id.to_string()) {
            return Err(Error::ValidationError("发票附件不能重复".into()));
        }
    }
    pending.ensure_all_used(&used)?;
    Ok(())
}

/// 在当前发票事务中登记文件并建立准确的发票附件关系。
pub(super) async fn persist(
    db: &Database,
    invoice_id: &str,
    ids: &[FileAssetId],
    pending: &dyn PendingAttachmentBatch,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    pending.persist(db, executor).await?;
    for id in ids {
        let asset = db
            .file_assets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("发票附件不存在".into()))?;
        asset.ensure_invoice_evidence()?;
        if asset.created_by != actor.id() {
            return Err(Error::Forbidden("请上传本人登记的有效发票图片或 PDF".into()));
        }
        let attachment = DocumentAttachment::new(
            DocumentAttachmentId::new(id_generator::next_id()),
            DocumentAttachmentData {
                document_id: BusinessDocumentId::new(invoice_id),
                file_asset_id: id.clone(),
                usage: AttachmentUsage::Attachment,
                created_by: actor.id().to_string(),
            },
        )?;
        db.document_attachments().create(&attachment, executor).await?;
    }
    Ok(())
}

/// 文件内容清单与业务请求共同构成精确幂等指纹。
pub(super) fn receipt(
    req: &CommitInvoiceRequest,
    actor_id: &str,
    pending: &dyn PendingAttachmentBatch,
) -> Result<CommandReceipt> {
    let manifest = pending.content_manifest();
    if manifest.is_empty() && !pending.is_empty() {
        return Err(Error::ValidationError("发票上传缺少真实文件内容清单".into()));
    }
    if manifest.is_empty() {
        Ok(CommandReceipt::from_payload(
            "sales-invoice-commit-",
            actor_id,
            "invoice.commit",
            "invoice",
            &req.idempotency_key,
            req,
        )?)
    } else {
        Ok(CommandReceipt::from_payload(
            "sales-invoice-commit-",
            actor_id,
            "invoice.commit",
            "invoice",
            &req.idempotency_key,
            &(req, manifest),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use erp_support::EmptyPendingAttachments;

    use super::*;
    #[test]
    fn old_commands_remain_valid_and_duplicate_or_unresolved_files_fail() {
        assert!(resolve(&mut [], &EmptyPendingAttachments).is_ok());
        assert!(resolve(&mut [FileAssetId::new("existing")], &EmptyPendingAttachments).is_ok());
        assert!(
            resolve(&mut [FileAssetId::new("same"), FileAssetId::new("same")], &EmptyPendingAttachments)
                .is_err()
        );
        assert!(resolve(&mut [FileAssetId::new("pending-file:missing")], &EmptyPendingAttachments).is_err());
    }

    #[test]
    fn command_fingerprint_binds_real_content_and_is_stable_across_reupload() {
        use erp_core::AccountKind;
        use erp_support::{
            PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass, SensitivityClass,
        };

        use crate::attachments::PendingFileAssets;
        let actor = AuditActor::new("actor".into(), "经办人".into(), AccountKind::Admin);
        let req: CommitInvoiceRequest = serde_json::from_value(serde_json::json!({"work_item_id":"task", "expected_task_version":"1", "invoice_id":"invoice", "expected_version":1, "invoice":null, "allocations":[], "idempotency_key":"same", "attachment_asset_ids":["pending-file:invoice"]})).unwrap();
        let batch = |digest: &str, storage: &str| {
            PendingFileAssets::prepare(
                vec![PendingFileAssetRequest {
                    reference: "pending-file:invoice".into(),
                    registration: RegisterFileAssetRequest {
                        storage_object_key: storage.into(),
                        file_name: "发票.pdf".into(),
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
        let first = receipt(&req, actor.id(), &batch("a", "object-first")).unwrap();
        let retry = receipt(&req, actor.id(), &batch("a", "object-retry")).unwrap();
        let changed = receipt(&req, actor.id(), &batch("b", "object-changed")).unwrap();
        assert_eq!(first.fingerprint(), retry.fingerprint());
        assert_ne!(first.fingerprint(), changed.fingerprint());
        use erp_finance::{Error as FinanceError, FinanceCommandReceipt};
        let fact =
            FinanceCommandReceipt::resource(&first, "invoice".to_string(), "event-1".to_string()).unwrap();
        assert_eq!(fact.resource_id(&retry).unwrap(), "invoice");
        assert!(matches!(fact.resource_id(&changed), Err(FinanceError::ConflictError(_))));

        let mut legacy = req.clone();
        legacy.attachment_asset_ids.clear();
        let old = CommandReceipt::from_payload(
            "sales-invoice-commit-",
            actor.id(),
            "invoice.commit",
            "invoice",
            &legacy.idempotency_key,
            &legacy,
        )
        .unwrap();
        assert_eq!(
            receipt(&legacy, actor.id(), &EmptyPendingAttachments).unwrap().fingerprint(),
            old.fingerprint()
        );
    }
}
