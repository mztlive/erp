//! 附件开票命令：服务端在同一授权事务内确定剩余批准金额并完成任务。
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_core::ids::InvoiceId;
use erp_finance::dto::receivable::CompleteInvoiceRequest;
use erp_finance::entity::receivable::{Invoice, SalesInvoiceRequest};
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_identity::SharedRbacService;
use erp_support::PendingAttachmentBatch;
use erp_workflow::ApprovalObjectReadPort;
use erp_workflow::service::work_item::expected_task_version;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::invoice::register_created_invoice_document;
use super::invoice_posting::{InvoicePostingAllocations, InvoicePostingInput, post_invoice_apply};
use super::{InvoiceWithAssetsResult, ReceivableProcess, invoice_attachments};
use crate::{Error, Result};

impl ReceivableProcess {
    /// 上传附件并按剩余批准金额完成一次开票。
    /// # 参数
    /// 任务及申请版本、附件命令、待持久化文件与当前账号。
    /// # 返回
    /// 已登记记录及本批上传是否消费；相同命令重放返回原结果。
    /// # 错误
    /// 附件缺失、权限、版本、额度或事务失败时拒绝；未知提交保留文件。
    pub async fn complete_invoice_with_assets(
        &self,
        mut req: CompleteInvoiceRequest,
        pending: Arc<dyn PendingAttachmentBatch>,
        actor: &AuditActor,
    ) -> Result<InvoiceWithAssetsResult> {
        req.validate()?;
        let receipt = evidence_receipt(&req, actor.id(), pending.as_ref())?;
        if let Some(id) = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&receipt, &mut NoTransaction)
            .await?
        {
            return Ok(InvoiceWithAssetsResult {
                view: self.finance.invoice_detail(&id).await?,
                assets_committed: false,
            });
        }
        invoice_attachments::resolve(&mut req.attachment_asset_ids, pending.as_ref())?;
        let expected = expected_task_version(&req.expected_task_version)?;
        let has_pending = !pending.is_empty();
        let revision = self.rbac.current_policy_revision().await?;
        let command = EvidenceCommand {
            db: self.db.clone(),
            rbac: self.rbac.clone(),
            object_read: Arc::clone(&self.object_read),
            req,
            pending,
            actor: actor.clone(),
            receipt: receipt.clone(),
            expected,
        };
        let result = self
            .rbac
            .clone()
            .run_authorized_policy_transaction(revision, move |executor| Box::pin(command.execute(executor)))
            .await;
        self.committed_invoice_view(result, &receipt, has_pending).await
    }
}

struct EvidenceCommand {
    db: Database,
    rbac: SharedRbacService,
    object_read: Arc<dyn ApprovalObjectReadPort>,
    req: CompleteInvoiceRequest,
    pending: Arc<dyn PendingAttachmentBatch>,
    actor: AuditActor,
    receipt: CommandReceipt,
    expected: u64,
}

impl EvidenceCommand {
    async fn request(&self, executor: &mut dyn Executor) -> Result<SalesInvoiceRequest> {
        let request = self
            .db
            .sales_invoice_requests()
            .find_for_task(self.req.work_item_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("此任务没有已批准的开票申请".into()))?;
        if request.base.version != self.req.expected_request_version {
            return Err(Error::ConflictError("开票申请额度已变化，请刷新后重新核对".into()));
        }
        Ok(request)
    }

    async fn execute(self, executor: &mut dyn Executor) -> Result<(String, bool)> {
        let db = &self.db;
        if let Some(id) = FinanceCommandReceiptService::new(db.clone())
            .committed_resource_id(&self.receipt, executor)
            .await?
        {
            return Ok((id, false));
        }
        let request = self.request(executor).await?;
        let mut invoice = Invoice::from_evidence(
            InvoiceId::new(id_generator::next_id()),
            request.counterparty_party_id.clone(),
            request.completion_amount(self.req.expected_request_version)?,
            self.actor.id(),
        )?;
        register_created_invoice_document(
            db,
            &self.rbac,
            self.object_read.as_ref(),
            &invoice,
            &self.actor,
            executor,
        )
        .await?;
        db.invoices().create(&invoice, executor).await?;
        post_invoice_apply(
            db,
            &mut invoice,
            InvoicePostingInput {
                work_item_id: &self.req.work_item_id,
                expected_task_version: self.expected,
                allocations: InvoicePostingAllocations::Evidence(&request.receivable_account_id),
                actor: &self.actor,
                action: "invoice.commit",
                command_receipt: Some(&self.receipt),
            },
            executor,
        )
        .await?;
        invoice_attachments::persist(
            db,
            &invoice.base.id,
            &self.req.attachment_asset_ids,
            self.pending.as_ref(),
            &self.actor,
            executor,
        )
        .await?;
        Ok((invoice.base.id, true))
    }
}

fn evidence_receipt(
    req: &CompleteInvoiceRequest,
    actor: &str,
    pending: &dyn PendingAttachmentBatch,
) -> Result<CommandReceipt> {
    let manifest = pending.content_manifest();
    if manifest.is_empty() && !pending.is_empty() {
        return Err(Error::ValidationError("发票上传缺少真实文件内容清单".into()));
    }
    Ok(CommandReceipt::from_payload(
        "sales-invoice-complete-",
        actor,
        "invoice.commit",
        "invoice",
        &req.idempotency_key,
        &(req, manifest),
    )?)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_support::{
        EmptyPendingAttachments, PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass,
        SensitivityClass,
    };

    use super::*;
    use crate::attachments::PendingFileAssets;

    #[test]
    fn retries_bind_task_versions_actor_and_file_content() {
        let actor = AuditActor::new("actor".into(), "经办人".into(), AccountKind::Admin);
        let req: CompleteInvoiceRequest = serde_json::from_value(serde_json::json!({
            "work_item_id":"task", "expected_task_version":"3", "expected_request_version":4,
            "attachment_asset_ids":["pending-file:invoice"], "idempotency_key":"same"
        }))
        .unwrap();
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
        let first = evidence_receipt(&req, actor.id(), &batch("a", "first")).unwrap();
        let retry = evidence_receipt(&req, actor.id(), &batch("a", "retry")).unwrap();
        let changed_file = evidence_receipt(&req, actor.id(), &batch("b", "changed")).unwrap();
        assert_eq!(first.fingerprint(), retry.fingerprint());
        assert_ne!(first.fingerprint(), changed_file.fingerprint());
        let stored =
            erp_finance::FinanceCommandReceipt::resource(&first, "invoice".into(), "event".into()).unwrap();
        assert_eq!(stored.resource_id(&retry).unwrap(), "invoice");
        assert!(stored.resource_id(&changed_file).is_err());
        let mut changed = req.clone();
        changed.expected_request_version += 1;
        assert_ne!(
            evidence_receipt(&changed, actor.id(), &batch("a", "first")).unwrap().fingerprint(),
            first.fingerprint()
        );
        changed = req.clone();
        changed.work_item_id = "another".to_string().into();
        assert_ne!(
            evidence_receipt(&changed, actor.id(), &batch("a", "first")).unwrap().fingerprint(),
            first.fingerprint()
        );
        assert!(
            invoice_attachments::resolve(&mut req.attachment_asset_ids.clone(), &EmptyPendingAttachments)
                .is_err()
        );
    }
}
