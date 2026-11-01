//! 拟核销安排独立校验来源，不参与已核销金额或余额汇总。

use std::collections::{HashMap, HashSet};

use erp_core::ids::ReceivableEntryId;
use erp_finance::entity::receivable::{CustomerReceiptStatus, PendingReceiptAllocation};
use erp_finance::repository::prelude::*;
use erp_finance::repository::{CustomerReceiptRow, ReceivableExt};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use persistence_core::Executor;

use super::{FundsAccess, FundsAuthorization, ScopedCustomerReceiptRow};
use crate::Result;

impl FundsAccess {
    /// 在当前事务内补齐未过账回款的拟核销安排。
    ///
    /// # 参数
    /// * `rows` - 已完成回款范围裁剪的返回行。
    /// * `sources` - 同一快照的原回款行。
    /// * `authorization` - 当前回款动作对应的销售来源授权。
    /// * `executor` - 调用方事务。
    /// # 返回
    /// 仅整单及全部拟核销来源可见时写入安排；受限或关联缺失时保持省略。
    /// # 错误
    /// 分录、子账或授权销售读取失败时返回错误。
    pub(super) async fn attach_receipt_pending<'a>(
        &self,
        rows: &mut [ScopedCustomerReceiptRow],
        sources: impl Iterator<Item = &'a CustomerReceiptRow> + Send,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let pending = sources
            .filter(|source| rows.iter().any(|row| row.id == source.id && !row.permission_limited))
            .map(|source| (source.id.clone(), pending_for_status(source)))
            .collect::<HashMap<_, _>>();
        let ids = pending
            .values()
            .flatten()
            .map(|line| line.receivable_entry_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let visible = self.visible_pending_entries(&ids, authorization, executor).await?;
        for row in rows {
            if let Some(lines) = pending.get(&row.id) {
                apply_pending(row, lines, &visible);
            }
        }
        Ok(())
    }

    /// 只查当前页引用的来源，并复用销售仓储的授权交集。
    async fn visible_pending_entries(
        &self,
        ids: &[ReceivableEntryId],
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let entries = self.db.receivable_entries().find_entries_by_ids(ids, executor).await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.receivable_account_id.to_string())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let accounts = self.db.receivable_accounts().find_accounts_by_ids(&account_ids, executor).await?;
        let order_ids = accounts
            .iter()
            .map(|account| account.sales_order_id.to_string())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut visible_orders = HashSet::new();
        for chunk in order_ids.chunks(500) {
            let orders = self
                .db
                .sales_orders()
                .scope_order_versions(chunk, &authorization.sales, &authorization.sales, executor)
                .await?;
            visible_orders.extend(orders.into_iter().map(|order| order.id));
        }
        let visible_accounts = accounts
            .into_iter()
            .filter(|account| visible_orders.contains(account.sales_order_id.as_ref()))
            .map(|account| account.base.id)
            .collect::<HashSet<_>>();
        Ok(entries
            .into_iter()
            .filter(|entry| visible_accounts.contains(entry.receivable_account_id.as_ref()))
            .map(|entry| entry.base.id)
            .collect())
    }
}

fn pending_for_status(source: &CustomerReceiptRow) -> Vec<PendingReceiptAllocation> {
    match source.status {
        CustomerReceiptStatus::Draft | CustomerReceiptStatus::InApproval => {
            source.pending_allocations.clone()
        },
        _ => Vec::new(),
    }
}

fn apply_pending(
    row: &mut ScopedCustomerReceiptRow,
    lines: &[PendingReceiptAllocation],
    visible: &HashSet<String>,
) {
    row.pending_allocations = (!row.permission_limited
        && lines.iter().all(|line| visible.contains(line.receivable_entry_id.as_ref())))
    .then(|| lines.to_vec());
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use super::*;

    fn source(status: CustomerReceiptStatus) -> CustomerReceiptRow {
        CustomerReceiptRow {
            id: "receipt".into(),
            receipt_no: "SK-1".into(),
            status,
            counterparty_party_id: "party".into(),
            customer_id: None,
            received_at: Instant::from_unix_secs(1),
            amount: "165".parse().unwrap(),
            bank_reference: None,
            version: 1,
            created_at: 1,
            pending_allocations: vec![
                PendingReceiptAllocation::new(ReceivableEntryId::new("entry"), "165".parse().unwrap())
                    .unwrap(),
            ],
        }
    }

    #[test]
    fn pending_keeps_amounts_unposted_and_serializes_sources() {
        let source = source(CustomerReceiptStatus::InApproval);
        let mut row = FundsAccess::cut_receipt_row(&source, &[], &[], true);
        apply_pending(&mut row, &pending_for_status(&source), &HashSet::from(["entry".into()]));
        assert_eq!(row.pending_allocations.as_ref().unwrap(), &source.pending_allocations);
        assert_eq!(row.allocated_total, Some("0".parse().unwrap()));
        assert_eq!(row.unallocated_amount, Some(source.amount));
        let json = serde_json::to_value(row).unwrap();
        assert_eq!(json["pending_allocations"][0]["receivable_entry_id"], "entry");
        assert_eq!(json["pending_allocations"][0]["allocated_amount"], "165");
    }

    #[test]
    fn pending_is_omitted_for_restricted_or_missing_sources() {
        let source = source(CustomerReceiptStatus::InApproval);
        for (whole, visible) in [(false, HashSet::from(["entry".into()])), (true, HashSet::new())] {
            let mut row = FundsAccess::cut_receipt_row(&source, &[], &[], whole);
            apply_pending(&mut row, &source.pending_allocations, &visible);
            assert!(serde_json::to_value(row).unwrap().get("pending_allocations").is_none());
        }
        let mut row = FundsAccess::cut_receipt_row(&source, &[], &[], true);
        let mut lines = source.pending_allocations.clone();
        lines.push(
            PendingReceiptAllocation::new(ReceivableEntryId::new("hidden"), "1".parse().unwrap()).unwrap(),
        );
        apply_pending(&mut row, &lines, &HashSet::from(["entry".into()]));
        assert!(row.pending_allocations.is_none());
    }

    #[test]
    fn posted_does_not_repeat_frozen_pending_and_empty_draft_is_known_empty() {
        for status in [CustomerReceiptStatus::Posted, CustomerReceiptStatus::Reversed] {
            assert!(pending_for_status(&source(status)).is_empty());
        }
        let source = source(CustomerReceiptStatus::Draft);
        assert_eq!(pending_for_status(&source), source.pending_allocations);
        let mut row = FundsAccess::cut_receipt_row(&source, &[], &[], true);
        apply_pending(&mut row, &[], &HashSet::new());
        assert_eq!(row.pending_allocations, Some(Vec::new()));
    }
}
