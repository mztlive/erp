//! 确认时重验当前授权、输入指纹、版本和商品资格，写入与回执保持原子。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::entity::sales_inquiry_import::{
    InquiryImportPreview, InquiryImportReceipt, InquiryImportResult,
};
use erp_sales::repository::extensions::SalesInquiryImportExt;
use erp_sales::service::sales_inquiry::SalesInquiryService;
use persistence_core::{Executor, NoTransaction};
use sha2::{Digest, Sha256};

use super::facts::sku_facts;
use super::{ConfirmInquiryImportCommand, SalesInquiryImportProcess};
use crate::audit::{persist_log, recover_command};
use crate::{Error, Result};

impl SalesInquiryImportProcess {
    /// 在一个事务内恢复原回执，或确认整份无错误预览。
    /// # 参数
    /// `actor`、`id` 为当前访问上下文，`command` 为版本化预览和幂等身份。
    /// # 返回
    /// 首次与相同命令重放均返回同一冻结结果。
    /// # 错误
    /// 越权、预览失效、异载荷、资格或版本变化均拒绝，不写入部分商品行。
    pub async fn confirm(
        &self,
        actor: &AuditActor,
        id: &str,
        mut command: ConfirmInquiryImportCommand,
    ) -> Result<InquiryImportResult> {
        command.command_key = command.command_key.trim().to_string();
        if command.command_key.is_empty() || command.command_key.len() > 128 {
            return Err(Error::ValidationError("操作号须为 1 至 128 字符".into()));
        }
        let result = self.confirm_transaction(actor, id, &command).await;
        match result {
            Err(error) if error.command_may_have_committed() => {
                recover_command(error, self.recover_confirm(actor, id, &command, &mut NoTransaction).await)
            },
            other => other,
        }
    }

    async fn confirm_transaction(
        &self,
        actor: &AuditActor,
        id: &str,
        command: &ConfirmInquiryImportCommand,
    ) -> Result<InquiryImportResult> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let command = command.clone();
        let revision = self.rbac.current_policy_revision().await?;
        self.rbac
            .run_authorized_policy_transaction(revision, move |executor| {
                Box::pin(async move { this.confirm_command(&actor, &id, &command, executor).await })
            })
            .await
    }

    async fn recover_confirm(
        &self,
        actor: &AuditActor,
        id: &str,
        command: &ConfirmInquiryImportCommand,
        executor: &mut dyn Executor,
    ) -> Result<Option<InquiryImportResult>> {
        let (command_id, fingerprint) = command_identity(actor.id(), id, command)?;
        let receipt =
            self.db.inquiry_import_receipts().find_by_id_including_deleted(&command_id, executor).await?;
        let Some(receipt) = receipt else {
            return Ok(None);
        };
        self.access().require_sales(actor, "import", id, executor).await?;
        Ok(Some(receipt.recover(&command_id, actor.id(), id, &command.preview_id, &fingerprint)?))
    }

    async fn confirm_command(
        &self,
        actor: &AuditActor,
        id: &str,
        command: &ConfirmInquiryImportCommand,
        executor: &mut dyn Executor,
    ) -> Result<InquiryImportResult> {
        if let Some(result) = self.recover_confirm(actor, id, command, executor).await? {
            return Ok(result);
        }
        let mut inquiry = self.require_import(actor, id, executor).await?;
        let (command_id, fingerprint) = command_identity(actor.id(), id, command)?;
        let mut preview = self
            .db
            .inquiry_import_previews()
            .find_by_id(&command.preview_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("导入预览不存在或不可用".into()))?;
        preview.ensure_confirmable(
            actor.id(),
            id,
            command.expected_version,
            &command.input_fingerprint,
            command.confirm_merge_duplicates,
            Instant::now().unix_secs(),
        )?;
        if preview.result.is_some() {
            return Err(Error::ConflictError("该预览已由其他确认命令处理".into()));
        }
        inquiry.ensure_version(command.expected_version)?;
        self.revalidate(&preview, executor).await?;
        self.apply_preview(actor, &mut inquiry, &mut preview, &command_id, fingerprint, executor).await
    }

    pub(super) async fn revalidate(
        &self,
        preview: &InquiryImportPreview,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let codes = preview.data.lines.iter().map(|line| line.sku_code.clone()).collect::<Vec<_>>();
        let facts = sku_facts(&self.db, &codes, executor).await?;
        for line in &preview.data.lines {
            let matches = facts.iter().any(|fact| {
                fact.sku_id == line.sku_id
                    && fact.sku_revision_id == line.sku_revision_id
                    && fact.sku_code == line.sku_code
                    && fact.unit_code == line.unit_code
                    && fact.quantity_scale == line.quantity_scale
                    && fact.fulfillment_kind == line.fulfillment_kind
            });
            if !matches {
                return Err(Error::ConflictError(
                    "商品不存在或不可选用，或当前修订及单位已变化，请重新预览".into(),
                ));
            }
        }
        Ok(())
    }

    async fn apply_preview(
        &self,
        actor: &AuditActor,
        inquiry: &mut SalesInquiry,
        preview: &mut InquiryImportPreview,
        command_id: &str,
        fingerprint: String,
        executor: &mut dyn Executor,
    ) -> Result<InquiryImportResult> {
        inquiry.save_draft(preview.expected_version, preview.data.clone())?;
        SalesInquiryService::new(self.db.clone()).save(inquiry, executor).await?;
        self.record_preview(actor, inquiry, preview, command_id, fingerprint, executor).await
    }

    pub(super) async fn record_preview(
        &self,
        actor: &AuditActor,
        inquiry: &SalesInquiry,
        preview: &mut InquiryImportPreview,
        command_id: &str,
        fingerprint: String,
        executor: &mut dyn Executor,
    ) -> Result<InquiryImportResult> {
        let result = InquiryImportResult {
            inquiry_id: inquiry.base.id.clone(),
            preview_id: preview.base.id.clone(),
            version: inquiry.base.version,
            line_count: inquiry.draft.lines.len(),
            allocation_count: inquiry.draft.lines.iter().map(|line| line.allocations.len()).sum(),
        };
        preview.result = Some(result.clone());
        self.db.inquiry_import_previews().update(preview, executor).await?;
        let receipt = InquiryImportReceipt {
            base: BaseModel::new(command_id.to_string()),
            actor_id: actor.id().into(),
            inquiry_id: inquiry.base.id.clone(),
            payload_fingerprint: fingerprint,
            result: result.clone(),
        };
        self.db.inquiry_import_receipts().create(&receipt, executor).await.map_err(receipt_write_error)?;
        let log =
            actor.clone().resource_log("sales_inquiry.import", "sales_inquiry", inquiry.base.id.clone())?;
        persist_log(&self.db, &log, executor).await?;
        Ok(result)
    }
}

fn receipt_write_error(error: persistence_core::Error) -> Error {
    if error.duplicate_index_name() == Some("uk_sales_inquiry_import_receipts_id") {
        Error::ReceiptDuplicate(error)
    } else {
        Error::from(error)
    }
}

fn command_identity(
    actor: &str,
    inquiry: &str,
    command: &ConfirmInquiryImportCommand,
) -> Result<(String, String)> {
    let key = serde_json::to_vec(&("sales_inquiry.import", actor, &command.command_key))
        .map_err(|_| Error::Internal("命令身份无法编码".into()))?;
    let payload =
        serde_json::to_vec(&(inquiry, command)).map_err(|_| Error::Internal("导入命令无法编码".into()))?;
    Ok((hex::encode(Sha256::digest(key)), hex::encode(Sha256::digest(payload))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_key_other_payload_keeps_identity_and_changes_fingerprint() {
        let mut command = ConfirmInquiryImportCommand {
            command_key: "key".into(),
            expected_version: 1,
            preview_id: "preview".into(),
            input_fingerprint: "a".repeat(64),
            confirm_merge_duplicates: true,
        };
        let original = command_identity("actor", "inquiry", &command).unwrap();
        command.confirm_merge_duplicates = false;
        let changed = command_identity("actor", "inquiry", &command).unwrap();
        assert_eq!(original.0, changed.0);
        assert_ne!(original.1, changed.1);
        assert_eq!(original.0, command_identity("actor", "other-inquiry", &command).unwrap().0);
        assert_ne!(original.0, command_identity("other-actor", "inquiry", &command).unwrap().0);
    }
}
