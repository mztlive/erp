//! 已授权完整子账的逾期余额；分录、正反核销与冲减来自同一事务。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use erp_finance::entity::receivable::{ReceiptAllocation, ReceivableEntry, ReceivableEntryOffset};

/// 汇总逾期增加分录，保留各分录实际核销和冲减归属。
///
/// # 参数
/// * `entries` - 同批全部已授权应收分录。
/// * `allocations` - 同批分录的完整正反核销流。
/// * `offsets` - 同批减少分录对应的冲减关系。
/// * `today` - 中国业务日期，到期当日不算逾期。
///
/// # 返回
/// 全部分录的逾期未收余额。
///
/// # 错误
/// 不返回错误，金额运算遵循 Amount 约定。
pub(super) fn total(
    entries: &[ReceivableEntry],
    allocations: &[ReceiptAllocation],
    offsets: &[ReceivableEntryOffset],
    today: BusinessDate,
) -> Amount {
    let mut settled = HashMap::<&str, Amount>::new();
    for allocation in allocations {
        let sum = settled.entry(allocation.receivable_entry_id.as_ref()).or_insert_with(Amount::zero);
        *sum = allocation.allocation_action.apply_to_net(*sum, allocation.allocated_amount);
    }
    let mut reduced = HashMap::<&str, Amount>::new();
    for offset in offsets {
        let sum = reduced.entry(offset.increase_entry_id.as_ref()).or_insert_with(Amount::zero);
        *sum = sum.checked_add(offset.offset_amount);
    }
    entries.iter().fold(Amount::zero(), |sum, entry| {
        sum.checked_add(entry.overdue_balance(
            today,
            settled.get(entry.base.id.as_str()).copied().unwrap_or_else(Amount::zero),
            reduced.get(entry.base.id.as_str()).copied().unwrap_or_else(Amount::zero),
        ))
    })
}
