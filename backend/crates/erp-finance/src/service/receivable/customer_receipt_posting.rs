//! Customer receipt ledger posting owned by finance.

use std::collections::{HashMap, HashSet};

use erp_core::common::time::Instant;
use erp_core::ids::{ReceiptAllocationId, ReceivableAccountId, ReceivableEntryId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::receivable::{CustomerReceipt, ReceivableAccount, ReceivableEntry, ReceivableFundsLedger};
use crate::repository::ReceivableExt;
use crate::service::receivable::mapping::map_ledger_error;
use crate::{Error, Result};

/// Settle an approval-validated receipt in the caller's current transaction.
///
/// Applies account deltas before receipt status and allocation writes, returning the
/// affected sales-order identities for the caller's later progress update.
/// Missing facts, cross-party allocation, ledger and balance failures propagate unchanged.
pub async fn settle_customer_receipt(
    db: &Database,
    receipt: &mut CustomerReceipt,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let existing = db
        .receipt_allocations()
        .find_allocations_by_receipts(&[receipt.base.id.clone().into()], executor)
        .await?;
    let pending = receipt.pending_allocations.clone();
    let mut ledger =
        ReceivableFundsLedger::new(receipt.base.id.clone().into(), receipt.amount, &existing, &pending)
            .map_err(map_ledger_error)?;

    let mut entry_ids: Vec<ReceivableEntryId> =
        pending.iter().map(|line| line.receivable_entry_id.clone()).collect();
    entry_ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    entry_ids.dedup();
    let entries = db.receivable_entries().find_entries_by_ids(&entry_ids, executor).await?;
    let entries_by_id: HashMap<&str, &ReceivableEntry> =
        entries.iter().map(|entry| (entry.base.id.as_str(), entry)).collect();
    let mut account_ids: Vec<ReceivableAccountId> =
        entries.iter().map(|entry| entry.receivable_account_id.clone()).collect();
    account_ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    account_ids.dedup();
    let accounts = db
        .receivable_accounts()
        .find_accounts_by_ids(&account_ids.iter().map(ToString::to_string).collect::<Vec<_>>(), executor)
        .await?;
    let accounts_by_id: HashMap<&str, &ReceivableAccount> =
        accounts.iter().map(|account| (account.base.id.as_str(), account)).collect();
    let mut checked_accounts: HashSet<ReceivableAccountId> = HashSet::new();
    let mut sales_order_ids = Vec::new();
    let allocation_ids: Vec<ReceiptAllocationId> =
        (0..pending.len()).map(|_| ReceiptAllocationId::new(next_id())).collect();
    let allocated_at = Instant::now();
    for (index, line) in pending.iter().enumerate() {
        let entry = entries_by_id
            .get(line.receivable_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        if checked_accounts.insert(entry.receivable_account_id.clone()) {
            let account = accounts_by_id
                .get(entry.receivable_account_id.as_ref())
                .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
            if account.counterparty_party_id != receipt.counterparty_party_id {
                return Err(Error::BusinessLogicError("禁止跨往来主体核销".to_string()));
            }
            sales_order_ids.push(account.sales_order_id.to_string());
        }
        ledger.apply(line, entry, allocation_ids[index].clone(), allocated_at).map_err(map_ledger_error)?;
    }
    let settlement_deltas = ledger.account_settlement_deltas();
    let settlement =
        db.receivable_accounts().apply_settlements_many(&settlement_deltas, actor_id, executor).await?;
    if !settlement.rejected.is_empty() {
        return Err(Error::BusinessLogicError("子账剩余开放余额不足，核销被拒绝".to_string()));
    }
    receipt.mark_posted()?;
    db.customer_receipts().update(receipt, executor).await?;
    db.receivable().create_receipt_allocations_many(ledger.new_allocations(), executor).await?;
    Ok(sales_order_ids)
}
