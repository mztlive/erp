//! 原命令授权事务内的应收子账投影；对外读取入口归 funds_scope。
use std::collections::HashMap;

use erp_core::ids::ReceivableEntryId;
use erp_core::money::Amount;
use erp_finance::dto::receivable::ReceivableEntryView;
use erp_finance::entity::receivable::{EntryDirection, ReceivableEntry};
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use persistence_core::Executor;

use super::ReceivableReadService;
use super::snapshot::{invoice_fact_views, load_receivable_snapshot, receipt_fact_views, zero_amount};
use crate::finance::command_snapshot;
use crate::finance::dto::ReceivableAccountView;
use crate::{Error, Result};

impl ReceivableReadService {
    /// 生成应收子账命令结果视图时，组合应收往来子账详情视图。
    ///
    /// # 参数
    /// * `id` - 本用例已授权的实际子账 ID。
    /// * `executor` - 完成该授权的原事务执行器。
    ///
    /// # 返回
    /// 返回完整台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    pub async fn receivable_account_view(
        &self,
        id: String,
        executor: &mut dyn Executor,
    ) -> Result<ReceivableAccountView> {
        command_snapshot(executor)?;
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(&id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;

        let snapshot = load_receivable_snapshot(&self.db, &account, executor).await?;
        let entry_views = self.account_entry_views(&snapshot.entries, executor).await?;
        let receipt_facts = receipt_fact_views(&snapshot);
        let invoice_facts = invoice_fact_views(&snapshot);

        Ok(ReceivableAccountView {
            id: account.base.id.clone(),
            sales_order_id: account.sales_order_id.to_string(),
            sales_order_no: snapshot.sales_order_no.clone(),
            sales_order_revision_no: snapshot.sales_order_revision_no,
            sales_order_snapshot_at: snapshot.sales_order_snapshot_at,
            account_seq: account.account_seq,
            source_sales_order_revision_id: account.source_sales_order_revision_id.to_string(),
            current_sales_order_revision_id: snapshot.current_sales_order_revision_id.clone(),
            customer_id: account.customer_id.to_string(),
            customer_name: snapshot.customer_name.clone(),
            counterparty_party_id: account.counterparty_party_id.to_string(),
            counterparty_party_name: snapshot.counterparty_party_name.clone(),
            gross_total: account.gross_total,
            settled_total: account.settled_total,
            open_total: account.open_total,
            invoiceable_total: account.invoiceable_total,
            invoiced_total: account.invoiced_total,
            open_invoiceable_total: account.open_invoiceable_total,
            status: account.stable.status(),
            version: account.base.version,
            account_domain_version: account.base.version.to_string(),
            receipt_facts,
            invoice_facts,
            created_at: account.base.created_at,
            entries: entry_views,
        })
    }
    // 组装应收子账命令结果时，在原快照补齐分录冲销额度并映射明细。
    async fn account_entry_views(
        &self,
        entries: &[ReceivableEntry],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntryView>> {
        let offsets = entries
            .iter()
            .filter(|entry| entry.direction == EntryDirection::Decrease)
            .map(|entry| entry.base.id.clone().into())
            .collect::<Vec<ReceivableEntryId>>();
        let mut offset_map: HashMap<String, Amount> = HashMap::new();
        for offset in self.db.receivable_entry_offsets().find_offsets_by_decreases(&offsets, executor).await?
        {
            let key = offset.increase_entry_id.to_string();
            let total = offset_map.entry(key).or_insert_with(zero_amount);
            *total = total.checked_add(offset.offset_amount);
        }

        let entry_views = entries
            .iter()
            .map(|entry| {
                let offset_total = offset_map.get(&entry.base.id).copied().unwrap_or_else(zero_amount);
                ReceivableEntryView {
                    receipt_available: None,
                    receipt_reserved: None,
                    id: entry.base.id.clone(),
                    entry_type: entry.entry_type,
                    direction: entry.direction,
                    amount: entry.amount,
                    due_date: entry.due_date,
                    source_document_id: entry.source_document_id.clone(),
                    source_sequence: entry.source_sequence,
                    posted_at: entry.posted_at,
                    offset_total,
                }
            })
            .collect();
        Ok(entry_views)
    }
}
