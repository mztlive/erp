//! 原命令授权事务内的应付子账投影；对外读取入口归 funds_scope。
use erp_finance::entity::payable::PayableAccount;
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use persistence_core::Executor;

use super::PayableReadService;
use super::display::{resolve_source_document_no, resolve_supplier_display};
use super::dto::{PayableAccountView, PayableEntryView, PaymentRecipientView};
use super::mapping::{
    payment_allocations_for_entries, payment_recipient_view, resolve_optional_payment_recipient_for_read,
};
use crate::finance::command_snapshot;
use crate::{Error, Result};

impl PayableReadService {
    // -----------------------------------------------------------------------
    // 应付往来子账
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配应付往来子账视图。
    ///
    /// # 参数
    /// * `id` - 本用例已授权的实际子账 ID。
    /// * `include_payment_recipient` - 是否加载任务/详情所需的当前收款账户。
    /// * `executor` - 完成该授权的原事务执行器。
    ///
    /// # 返回
    /// 返回完整应付台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    pub async fn payable_account_view(
        &self,
        id: String,
        include_payment_recipient: bool,
        executor: &mut dyn Executor,
    ) -> Result<PayableAccountView> {
        command_snapshot(executor)?;
        let account = self
            .db
            .payable_accounts()
            .find_by_id(&id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
        let source_document_no = resolve_source_document_no(&self.db, &account, executor).await?;
        let entries = self.detail_entries(&account, source_document_no.as_deref(), executor).await?;
        let payment_allocations = payment_allocations_for_entries(&self.db, &entries, executor).await?;
        let (supplier_no, supplier_name) =
            resolve_supplier_display(&self.db, account.supplier_id.as_ref(), executor).await?;
        let payment_recipient = self.detail_recipient(&account, include_payment_recipient, executor).await?;
        Ok(PayableAccountView {
            id: account.base.id.clone(),
            source_document_id: account.source_document_id,
            source_document_no,
            supplier_id: account.supplier_id.to_string(),
            supplier_no,
            supplier_name,
            payment_recipient,
            source_type: account.source_type,
            gross_total: account.gross_total,
            settled_total: account.settled_total,
            open_total: account.open_total,
            invoiceable_total: account.invoiceable_total,
            invoiced_total: account.invoiced_total,
            open_invoiceable_total: account.open_invoiceable_total,
            status: account.stable.status(),
            version: account.base.version,
            created_at: account.base.created_at,
            entries,
            payment_allocations,
        })
    }

    /// 读取详情分录并补全与子账相同来源的业务单号。
    async fn detail_entries(
        &self,
        account: &PayableAccount,
        source_document_no: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntryView>> {
        Ok(self
            .db
            .payable_entries()
            .find_entries_by_account(&account.base.id.clone().into(), executor)
            .await?
            .into_iter()
            .map(|entry| PayableEntryView {
                id: entry.base.id,
                entry_type: entry.entry_type,
                direction: entry.direction,
                amount: entry.amount,
                due_date: entry.due_date,
                source_document_no: (entry.source_document_id == account.source_document_id)
                    .then(|| source_document_no.map(str::to_owned))
                    .flatten(),
                source_document_id: entry.source_document_id,
                source_sequence: entry.source_sequence,
                posted_at: entry.posted_at,
            })
            .collect())
    }

    /// 为详情按原只读规则解析当前收款账户。
    async fn detail_recipient(
        &self,
        account: &PayableAccount,
        include: bool,
        executor: &mut dyn Executor,
    ) -> Result<Option<PaymentRecipientView>> {
        if !include {
            return Ok(None);
        }
        Ok(resolve_optional_payment_recipient_for_read(&self.db, &account.supplier_id, executor)
            .await?
            .as_ref()
            .map(payment_recipient_view))
    }
}
