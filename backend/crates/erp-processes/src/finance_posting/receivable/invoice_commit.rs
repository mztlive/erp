//! 销项发票提交事务：领域过账及文件关联沿用同一个执行器。
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_core::ids::{FileAssetId, InvoiceId, WorkItemId};
use erp_finance::dto::receivable::CreateInvoiceRequest;
use erp_finance::entity::receivable::sales_invoice_allocation_plan::SalesInvoiceAllocationLine;
use erp_finance::entity::receivable::{Invoice, InvoiceData, InvoiceStatus};
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::receivable::invoice_commit::{PreparedInvoiceCommit, ensure_sales_invoice};
use erp_finance::service::receivable::mapping::{ensure_expected_version, zero_amount};
use erp_identity::SharedRbacService;
use erp_support::PendingAttachmentBatch;
use erp_workflow::ApprovalObjectReadPort;
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use super::invoice::register_created_invoice_document;
use super::invoice_posting::{InvoicePostingInput, post_invoice_apply};
use crate::{Error, Result};

/// 本次已规范化的发票事务输入，不保存 S3 字节。
pub(super) struct InvoiceCommitTransaction {
    pub db: Database,
    pub rbac: SharedRbacService,
    pub object_read: Arc<dyn ApprovalObjectReadPort>,
    pub prepared: PreparedInvoiceCommit,
    pub pending: Arc<dyn PendingAttachmentBatch>,
    pub attachment_ids: Vec<FileAssetId>,
    pub actor: AuditActor,
    pub receipt: CommandReceipt,
    pub work_item_id: WorkItemId,
    pub expected_task_version: u64,
}

impl InvoiceCommitTransaction {
    /// 在入口已开启的事务内读取、过账及关联全部发票事实。
    ///
    /// # 参数
    /// * `executor` - 入口授权事务使用的同一执行器。
    /// # 返回
    /// 返回原发票 ID 与本事务是否首次执行；重放不会消费新上传对象。
    /// # 错误
    /// 回执、授权、版本、发票、金额、任务或附件失败时停止并返回原错误。
    pub async fn execute(self, executor: &mut dyn Executor) -> Result<(String, bool)> {
        if let Some(id) = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&self.receipt, executor)
            .await?
        {
            return Ok((id, false));
        }
        let (mut invoice, lines) = self.load_invoice(executor).await?;
        self.ensure_draft(&invoice, executor).await?;
        post_invoice_apply(
            &self.db,
            &mut invoice,
            InvoicePostingInput {
                work_item_id: &self.work_item_id,
                expected_task_version: self.expected_task_version,
                plan_lines: &lines,
                actor: &self.actor,
                action: "invoice.commit",
                command_receipt: Some(&self.receipt),
            },
            executor,
        )
        .await?;
        super::invoice_attachments::persist(
            &self.db,
            &invoice.base.id,
            &self.attachment_ids,
            self.pending.as_ref(),
            &self.actor,
            executor,
        )
        .await?;
        Ok((invoice.base.id, true))
    }

    /// 新建路径登记无审批单据；已有草稿路径重验版本及销项方向。
    async fn load_invoice(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<(Invoice, Vec<SalesInvoiceAllocationLine>)> {
        match &self.prepared {
            PreparedInvoiceCommit::New { invoice, allocations } => {
                invoice.validate()?;
                let invoice = new_invoice(invoice, self.actor.id())?;
                register_created_invoice_document(
                    &self.db,
                    &self.rbac,
                    self.object_read.as_ref(),
                    &invoice,
                    &self.actor,
                    executor,
                )
                .await?;
                self.db.invoices().create(&invoice, executor).await?;
                Ok((invoice, allocations.clone()))
            },
            PreparedInvoiceCommit::Existing { invoice_id, expected_version, allocations } => {
                let invoice = self
                    .db
                    .invoices()
                    .find_by_id(invoice_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("发票不存在".into()))?;
                ensure_expected_version(invoice.base.version, *expected_version)?;
                ensure_sales_invoice(&invoice)?;
                Ok((invoice, allocations.clone()))
            },
        }
    }

    /// 保持过账前草稿状态和规范化发票号码的原首错顺序。
    async fn ensure_draft(&self, invoice: &Invoice, executor: &mut dyn Executor) -> Result<()> {
        if invoice.stable.status() != InvoiceStatus::Draft {
            return Err(Error::ConflictError("发票已登记，请勿重复提交".into()));
        }
        let duplicate = self
            .db
            .invoices()
            .find_by_direction_and_normalized_no(invoice.invoice_direction, &invoice.normalized_no, executor)
            .await?;
        if duplicate.as_ref().is_some_and(|other| other.base.id != invoice.base.id) {
            return Err(Error::ConflictError("发票号码已登记，请勿重复提交".into()));
        }
        Ok(())
    }
}

/// 从已校验请求构造新发票；金额和单据身份语义沿用发票实体。
fn new_invoice(req: &CreateInvoiceRequest, actor_id: &str) -> Result<Invoice> {
    Ok(Invoice::new(
        InvoiceId::new(id_generator::next_id()),
        InvoiceData {
            invoice_direction: req.invoice_direction,
            invoice_kind: req.invoice_kind,
            party_id: req.party_id.clone(),
            invoice_code: req.invoice_code.clone(),
            invoice_no: req.invoice_no.clone(),
            invoice_date: req.invoice_date,
            gross_amount: req.gross_amount,
            net_amount: req.net_amount,
            tax_amount: req.tax_amount,
            rounding_adjustment_amount: req.rounding_adjustment_amount.unwrap_or(zero_amount()),
            rounding_reason: req.rounding_reason.clone(),
            original_invoice_id: None,
        },
        actor_id,
    )?)
}
