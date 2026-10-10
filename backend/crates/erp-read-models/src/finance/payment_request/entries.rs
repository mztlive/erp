//! 同一付款申请来源账户的有效核销分录，不展开其他采购的付款分配。

use std::collections::BTreeMap;

use erp_core::ids::{PayableAccountId, PayableEntryId};
use erp_core::money::Amount;
use erp_finance::entity::payable::{
    AllocationAction, EntryDirection, PayableEntry, PayableEntryOffset, PaymentAllocation,
};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use persistence_core::Executor;

use super::{PaymentRequestPayableEntryView, PurchasePaymentRequestReadService};
use crate::{Error, Result};

impl PurchasePaymentRequestReadService {
    pub(super) async fn payable_entries(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentRequestPayableEntryView>> {
        let entries = self
            .db
            .payable_entries()
            .find_entries_by_account(&PayableAccountId::new(account_id), executor)
            .await?;
        let ids = entries
            .iter()
            .filter(|entry| entry.direction == EntryDirection::Increase)
            .map(|entry| PayableEntryId::new(&entry.base.id))
            .collect::<Vec<_>>();
        let allocations = self.db.payment_allocations().find_allocations_by_entries(&ids, executor).await?;
        let offsets = self.db.payable_entry_offsets().find_offsets_by_increases(&ids, executor).await?;
        entry_views(&entries, &allocations, &offsets)
    }
}

fn entry_views(
    entries: &[PayableEntry],
    allocations: &[PaymentAllocation],
    offsets: &[PayableEntryOffset],
) -> Result<Vec<PaymentRequestPayableEntryView>> {
    let mut settled = BTreeMap::new();
    let mut decreased = BTreeMap::new();
    for allocation in allocations {
        let amount = match allocation.allocation_action {
            AllocationAction::Apply => allocation.allocated_amount,
            AllocationAction::Reverse => Amount::zero().checked_sub(allocation.allocated_amount),
        };
        add(&mut settled, allocation.payable_entry_id.as_ref(), amount)?;
    }
    for offset in offsets {
        add(&mut decreased, offset.increase_entry_id.as_ref(), offset.offset_amount)?;
    }
    let mut views = Vec::new();
    for entry in entries.iter().filter(|entry| entry.direction == EntryDirection::Increase) {
        let settled_total = settled.get(entry.base.id.as_str()).copied().unwrap_or_else(Amount::zero);
        let offset_total = decreased.get(entry.base.id.as_str()).copied().unwrap_or_else(Amount::zero);
        let open_total = entry.amount.checked_sub(settled_total).checked_sub(offset_total);
        if open_total < Amount::zero() || settled_total < Amount::zero() {
            return Err(Error::ConflictError("付款申请来源分录余额异常".into()));
        }
        views.push(PaymentRequestPayableEntryView {
            id: entry.base.id.clone(),
            amount: entry.amount,
            settled_total,
            offset_total,
            open_total,
        });
    }
    views.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(views)
}

fn add(totals: &mut BTreeMap<String, Amount>, id: &str, amount: Amount) -> Result<()> {
    let value = totals.entry(id.to_owned()).or_insert_with(Amount::zero);
    let decimal = value
        .to_decimal()
        .checked_add(amount.to_decimal())
        .ok_or_else(|| Error::ConflictError("付款申请分录汇总溢出".into()))?;
    *value = Amount::try_from(decimal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{PaymentAllocationId, SupplierPaymentId};
    use erp_finance::entity::payable::PayableEntryType;

    use super::*;

    fn entry(id: &str, direction: EntryDirection) -> PayableEntry {
        PayableEntry {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            payable_account_id: PayableAccountId::new("account"),
            entry_type: PayableEntryType::Original,
            direction,
            amount: "100.00".parse().unwrap(),
            due_date: BusinessDate::today(),
            source_fact_type: "purchase_order".into(),
            source_document_id: "purchase".into(),
            source_revision_id: "revision".into(),
            source_sequence: 1,
            posted_at: Instant::now(),
        }
    }

    fn allocation(action: AllocationAction, amount: &str) -> PaymentAllocation {
        PaymentAllocation {
            base: BaseModel::fake(),
            supplier_payment_id: SupplierPaymentId::new("payment"),
            payable_entry_id: PayableEntryId::new("e-1"),
            allocation_seq: 1,
            allocation_action: action,
            allocated_amount: amount.parse().unwrap(),
            allocated_at: Instant::now(),
            reverses_allocation_id: (action == AllocationAction::Reverse)
                .then(|| PaymentAllocationId::new("original")),
        }
    }

    #[test]
    fn entry_balances_combine_effective_payments_reversals_and_formal_decreases() {
        let offset = PayableEntryOffset {
            base: BaseModel::fake(),
            decrease_entry_id: PayableEntryId::new("decrease"),
            increase_entry_id: PayableEntryId::new("e-1"),
            offset_sequence: 1,
            offset_amount: "10.00".parse().unwrap(),
        };
        let rows = entry_views(
            &[
                entry("e-2", EntryDirection::Increase),
                entry("e-1", EntryDirection::Increase),
                entry("decrease", EntryDirection::Decrease),
            ],
            &[allocation(AllocationAction::Apply, "60"), allocation(AllocationAction::Reverse, "20")],
            &[offset],
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "e-1");
        assert_eq!(rows[0].settled_total, "40".parse().unwrap());
        assert_eq!(rows[0].open_total, "50".parse().unwrap());
        assert_eq!(rows[1].open_total, "100".parse().unwrap());
    }

    #[test]
    fn entry_balances_reject_negative_open_or_unmatched_reverse_instead_of_clamping() {
        let entries = [entry("e-1", EntryDirection::Increase)];
        assert!(matches!(
            entry_views(&entries, &[allocation(AllocationAction::Apply, "101")], &[]),
            Err(Error::ConflictError(_))
        ));
        assert!(matches!(
            entry_views(&entries, &[allocation(AllocationAction::Reverse, "1")], &[]),
            Err(Error::ConflictError(_))
        ));
        assert!(entry_views(&[], &[], &[]).unwrap().is_empty());
    }
}
