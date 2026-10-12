//! 回款草稿与命令回执的原子保存。
use application_core::{AuditActor, CommandReceipt};
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::ids::CustomerReceiptId;
use erp_finance::dto::receivable::{CreateCustomerReceiptRequest, SaveCustomerReceiptDraftRequest};
use erp_finance::entity::receivable::{
    CustomerReceipt, CustomerReceiptData, CustomerReceiptUpdate, EntryDirection,
};
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::receivable::customer_receipt_commit::convert_allocations;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_read_models::finance::dto::CustomerReceiptView;
use erp_workflow::service::approval::approval_action_roles_with_executor;
use id_generator::next_id;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::ReceivableProcess;
use super::customer_receipt_posting::create_new_commit_records;
use super::draft_read::{ensure_receipt_owner, receipt_view_with_actions};
use super::draft_update::ensure_receipt_edit_authorized;
use crate::adapters::workflow::workflow_auth;
use crate::audit::persist_log;
use crate::{Error, Result};

impl ReceivableProcess {
    /// 在同一事务保存完整回款草稿、拟核销分配与幂等回执。
    /// # 参数
    /// * `req` - 原单版本、到账字段、分配与稳定操作号。
    /// * `actor` - 当前已认证操作人。
    /// # 返回
    /// 返回原单当前状态；相同操作重试不重复创建或更新。
    /// # 错误
    /// 无权、异载荷重试、状态或版本冲突、非法来源时拒绝。
    pub async fn save_customer_receipt_draft(
        &self,
        req: SaveCustomerReceiptDraftRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        req.receipt.validate()?;
        req.validate_identity()?;
        let command = CommandReceipt::from_payload(
            "customer-receipt-draft-",
            actor.id(),
            "customer_receipt.save_draft",
            "customer_receipt",
            &req.idempotency_key,
            &req,
        )?;
        let process = Self::new(self.db.clone()).with_object_read(self.object_read.clone());
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.save_draft_command(req, &actor, &command, executor).await })
            })
            .await
    }

    async fn save_draft_command(
        &self,
        req: SaveCustomerReceiptDraftRequest,
        actor: &AuditActor,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<CustomerReceiptView> {
        let receipts = FinanceCommandReceiptService::new(self.db.clone());
        if let Some(id) = receipts.committed_resource_id(command, executor).await? {
            ensure_receipt_edit_authorized(&self.db, &self.rbac, actor, &id, executor).await?;
            let receipt = self.draft_receipt(&id, executor).await?;
            ensure_receipt_owner(&receipt, actor.id())?;
            return receipt_view_with_actions(&self.db, &self.rbac, actor, &id, executor).await;
        }
        let mut receipt = self.prepare_saved_draft(&req, actor, executor).await?;
        receipt.save_allocations(convert_allocations(&req.allocations)?)?;
        self.validate_draft_sources(&receipt, executor).await?;
        self.db.customer_receipts().update(&mut receipt, executor).await?;
        ensure_receipt_edit_authorized(&self.db, &self.rbac, actor, &receipt.base.id, executor).await?;
        let audit = draft_audit(actor, &receipt.base.id, req.receipt_id.as_deref())?;
        persist_log(&self.db, &audit, executor).await?;
        receipts.save_resource(command, receipt.base.id.clone(), audit.base.id, executor).await?;
        receipt_view_with_actions(&self.db, &self.rbac, actor, &receipt.base.id, executor).await
    }

    async fn prepare_saved_draft(
        &self,
        req: &SaveCustomerReceiptDraftRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerReceipt> {
        let data = &req.receipt;
        if let Some(id) = &req.receipt_id {
            ensure_receipt_edit_authorized(&self.db, &self.rbac, actor, id, executor).await?;
            let mut receipt = self.draft_receipt(id, executor).await?;
            receipt.ensure_draft_editor(actor.id())?;
            ensure_expected_version(receipt.base.version, req.expected_version.unwrap_or_default())?;
            if receipt.receipt_no != data.receipt_no
                || receipt.counterparty_party_id != data.counterparty_party_id
                || receipt.customer_id != data.customer_id
            {
                return Err(Error::ValidationError("保存草稿不得改变原单身份或往来主体".into()));
            }
            receipt.update(CustomerReceiptUpdate {
                amount: Some(data.amount),
                received_at: Some(data.received_at),
                bank_reference: Some(data.bank_reference.clone().unwrap_or_default()),
            })?;
            return Ok(receipt);
        }
        self.create_saved_draft(data, actor, executor).await
    }

    async fn create_saved_draft(
        &self,
        data: &CreateCustomerReceiptRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerReceipt> {
        let auth = workflow_auth(self.db.clone(), self.rbac.clone());
        if approval_action_roles_with_executor(&auth, actor, "customer_receipt:create", executor)
            .await?
            .is_empty()
        {
            return Err(Error::Forbidden("当前账号缺少回款登记权限".into()));
        }
        let receipt = CustomerReceipt::new(
            CustomerReceiptId::new(next_id()),
            CustomerReceiptData {
                receipt_no: data.receipt_no.clone(),
                counterparty_party_id: data.counterparty_party_id.clone(),
                customer_id: data.customer_id.clone(),
                amount: data.amount,
                received_at: data.received_at,
                bank_reference: data.bank_reference.clone(),
            },
            actor.id(),
        )?;
        let (receipt, _) = create_new_commit_records(
            &self.db,
            &self.rbac,
            self.object_read.as_ref(),
            receipt,
            actor,
            executor,
        )
        .await?;
        Ok(receipt)
    }

    async fn draft_receipt(&self, id: &str, executor: &mut dyn Executor) -> Result<CustomerReceipt> {
        self.db
            .customer_receipts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户回款单不存在".into()))
    }

    async fn validate_draft_sources(
        &self,
        receipt: &CustomerReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = receipt
            .pending_allocations
            .iter()
            .map(|line| line.receivable_entry_id.clone())
            .collect::<Vec<_>>();
        let entries = self.db.receivable_entries().find_entries_by_ids(&ids, executor).await?;
        if entries.len() != ids.len() {
            return Err(Error::ValidationError("核销应收分录不存在".into()));
        }
        let ids = entries.iter().map(|entry| entry.receivable_account_id.to_string()).collect::<Vec<_>>();
        let accounts = self.db.receivable_accounts().find_accounts_by_ids(&ids, executor).await?;
        for entry in entries {
            if entry.direction != EntryDirection::Increase {
                return Err(Error::ValidationError("核销只能选择正向应收分录".into()));
            }
            let account = accounts
                .iter()
                .find(|account| account.base.id == entry.receivable_account_id.as_ref())
                .ok_or_else(|| Error::NotFound("应收子账不存在".into()))?;
            if account.counterparty_party_id != receipt.counterparty_party_id {
                return Err(Error::ValidationError("禁止跨往来主体核销".into()));
            }
        }
        Ok(())
    }
}

/// 草稿保存沿用已登记的新建/修改事件，不向审计写入表单原文。
fn draft_audit(actor: &AuditActor, id: &str, existing_id: Option<&str>) -> Result<AuditLog> {
    let action = if existing_id.is_some() { "customer_receipt.update" } else { "customer_receipt.create" };
    Ok(actor.clone().resource_log(action, "customer_receipt", id.to_owned())?)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;

    #[test]
    fn draft_save_builds_registered_create_and_update_events() {
        let actor = AuditActor::new("actor".into(), "fukuan".into(), AccountKind::Admin);
        let created = draft_audit(&actor, "receipt", None).unwrap();
        let updated = draft_audit(&actor, "receipt", Some("receipt")).unwrap();
        assert_eq!(created.action, "customer_receipt.create");
        assert_eq!(updated.action, "customer_receipt.update");
        assert_eq!(created.resource_id.as_deref(), Some("receipt"));
        assert!(created.structured_event.is_some());
        assert!(updated.structured_event.is_some());
        assert!(draft_audit(&actor, "", None).is_err());
    }
}
