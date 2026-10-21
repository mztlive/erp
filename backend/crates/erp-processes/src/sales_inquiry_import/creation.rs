//! 客户、预览、草稿和回执在一次授权事务中完成绑定。

use application_core::{AuditActor, CommandReceipt};
use erp_core::common::time::Instant;
use erp_sales::entity::sales_inquiry_import::InquiryImportResult;
use erp_sales::repository::extensions::SalesInquiryImportExt;
use persistence_core::{Executor, NoTransaction};

use super::{CreateInquiryImportCommand, SalesInquiryImportProcess};
use crate::audit::recover_command;
use crate::sales_inquiry::SalesInquiryProcess;
use crate::sales_inquiry::dto::CreateInquiryCommand;
use crate::{Error, Result};

impl SalesInquiryImportProcess {
    /// 将客户专用预览一次性创建为草稿；相同命令重试返回原结果。
    /// # 参数
    /// `actor` 为当前销售，`command` 绑定客户、预览、文件指纹及合并确认。
    /// # 返回
    /// 新草稿与已保存的导入结果。
    /// # 错误
    /// 权限、客户、预览或商品失效时整体拒绝；未知提交只恢复原回执。
    pub async fn create_from_preview(
        &self,
        actor: &AuditActor,
        mut command: CreateInquiryImportCommand,
    ) -> Result<InquiryImportResult> {
        command.command_key = command.command_key.trim().into();
        let receipt = creation_identity(actor.id(), &command)?;
        let this = self.clone();
        let actor_copy = actor.clone();
        let request = command.clone();
        let identity = receipt.clone();
        let revision = self.rbac.current_policy_revision().await?;
        let result = self
            .rbac
            .run_authorized_policy_transaction(revision, move |executor| {
                Box::pin(async move { this.create_command(&actor_copy, &request, &identity, executor).await })
            })
            .await;
        match result {
            Err(error) if error.command_may_have_committed() => recover_command(
                error,
                self.recover_creation(actor, &command, &receipt, &mut NoTransaction).await,
            ),
            other => other,
        }
    }

    async fn recover_creation(
        &self,
        actor: &AuditActor,
        command: &CreateInquiryImportCommand,
        identity: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<InquiryImportResult>> {
        let receipt =
            self.db.inquiry_import_receipts().find_by_id_including_deleted(identity.id(), executor).await?;
        let Some(receipt) = receipt else { return Ok(None) };
        self.creation_header(actor, &command.customer_id, executor).await?;
        let inquiry = self.require_import(actor, &receipt.inquiry_id, executor).await?;
        if inquiry.customer_id != command.customer_id {
            return Err(Error::ConflictError("同一操作号已用于其他客户的导入".into()));
        }
        Ok(Some(receipt.recover(
            identity.id(),
            actor.id(),
            &receipt.inquiry_id,
            &command.preview_id,
            identity.fingerprint().digest_hex(),
        )?))
    }

    async fn create_command(
        &self,
        actor: &AuditActor,
        command: &CreateInquiryImportCommand,
        identity: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<InquiryImportResult> {
        if let Some(result) = self.recover_creation(actor, command, identity, executor).await? {
            return Ok(result);
        }
        let mut data = self.creation_header(actor, &command.customer_id, executor).await?;
        let mut preview = self
            .db
            .inquiry_import_previews()
            .find_by_id(&command.preview_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("导入预览不存在或不可用".into()))?;
        preview.ensure_creatable(
            actor.id(),
            &command.customer_id,
            &command.input_fingerprint,
            command.confirm_merge_duplicates,
            Instant::now().unix_secs(),
        )?;
        self.revalidate(&preview, executor).await?;
        data.draft = preview.data.clone();
        let inquiry = SalesInquiryProcess::new(self.db.clone(), self.rbac.clone())
            .create_inquiry(
                actor,
                &CreateInquiryCommand { command_key: command.command_key.clone(), data },
                executor,
            )
            .await?;
        self.record_preview(
            actor,
            &inquiry,
            &mut preview,
            identity.id(),
            identity.fingerprint().digest_hex().into(),
            executor,
        )
        .await
    }
}

fn creation_identity(actor: &str, command: &CreateInquiryImportCommand) -> Result<CommandReceipt> {
    if command.command_key.is_empty() || command.command_key.len() > 128 {
        return Err(Error::ValidationError("操作号须为 1 至 128 字符".into()));
    }
    Ok(CommandReceipt::from_payload(
        "sales-inquiry-import-create:",
        actor,
        "import_create",
        "sales_inquiry",
        &command.command_key,
        command,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_retry_binds_customer_file_and_preview_without_changing_command_identity() {
        let mut command = CreateInquiryImportCommand {
            command_key: "key".into(),
            customer_id: "customer".into(),
            preview_id: "preview".into(),
            input_fingerprint: "a".repeat(64),
            confirm_merge_duplicates: false,
        };
        let original = creation_identity("actor", &command).unwrap();
        let repeated = creation_identity("actor", &command).unwrap();
        assert_eq!(original.id(), repeated.id());
        assert_eq!(original.fingerprint().digest_hex(), repeated.fingerprint().digest_hex());
        command.customer_id = "other".into();
        let changed = creation_identity("actor", &command).unwrap();
        assert_eq!(original.id(), changed.id());
        assert_ne!(original.fingerprint().digest_hex(), changed.fingerprint().digest_hex());
        command.command_key.clear();
        assert!(creation_identity("actor", &command).is_err());
        command.command_key = "a".repeat(129);
        assert!(creation_identity("actor", &command).is_err());
    }
}
