//! 回款候选与提交共用的分录、子账双重额度规则。

use std::collections::{HashMap, HashSet};

use erp_core::ids::PartyId;
use erp_core::money::Amount;

use super::{
    AllocationAction, CustomerReceipt, EntryDirection, PendingReceiptAllocation, ReceiptAllocation,
    ReceivableEntry, ReceivableEntryOffset, checked_add_amount, checked_sub_amount,
};
use crate::{Error, Result};

/// 单个子账的当前余额；审批占用不计入已收。
pub struct ReceiptAccountCapacity {
    /// 子账身份。
    pub id: String,
    /// 当前结算主体。
    pub party_id: PartyId,
    /// 未核销余额；构造额度后扣除全部有效占用。
    pub open: Amount,
}

/// 单个应收项目的真实核销依据。
#[derive(Debug, Clone, Copy)]
pub struct ReceiptEntryCapacity {
    /// 实际净核销金额。
    pub settled: Amount,
    /// 应收冲减金额。
    pub offset: Amount,
    /// 其他审批中拟核销金额。
    pub reserved: Amount,
    /// 当前可新增核销金额。
    pub available: Amount,
}

/// 同一快照内的应收项目与子账可用额度。
pub struct ReceiptCapacity {
    entries: HashMap<String, (String, ReceiptEntryCapacity)>,
    accounts: HashMap<String, ReceiptAccountCapacity>,
}

impl ReceiptCapacity {
    /// 从实际核销、冲减和其他审批中占用构造额度。
    ///
    /// # 参数
    /// * `accounts` - 完整子账当前未收余额。
    /// * `entries` - 上述子账的全部分录。
    /// * `allocations` - 全部分录的实际正反核销。
    /// * `offsets` - 全部冲减关系。
    /// * `reserved` - 按分录汇总的其他审批中拟核销金额。
    ///
    /// # 返回
    /// 返回统一额度；负余额按零展示。
    ///
    /// # 错误
    /// 金额运算溢出或分录缺少所属子账时返回错误。
    pub fn new(
        accounts: Vec<ReceiptAccountCapacity>,
        entries: &[ReceivableEntry],
        allocations: &[ReceiptAllocation],
        offsets: &[ReceivableEntryOffset],
        reserved: &[PendingReceiptAllocation],
    ) -> Result<Self> {
        let mut result = Self {
            entries: HashMap::new(),
            accounts: accounts.into_iter().map(|account| (account.id.clone(), account)).collect(),
        };
        let mut settled = HashMap::new();
        for line in allocations {
            let sum = settled.entry(line.receivable_entry_id.as_ref()).or_insert(Amount::zero());
            *sum = match line.allocation_action {
                AllocationAction::Apply => checked_add_amount(*sum, line.allocated_amount)?,
                AllocationAction::Reverse => checked_sub_amount(*sum, line.allocated_amount)?,
            };
        }
        let mut reduced = HashMap::new();
        for offset in offsets {
            let sum = reduced.entry(offset.increase_entry_id.as_ref()).or_insert(Amount::zero());
            *sum = checked_add_amount(*sum, offset.offset_amount)?;
        }
        let mut pending = HashMap::new();
        for line in reserved {
            let sum = pending.entry(line.receivable_entry_id.as_ref()).or_insert(Amount::zero());
            *sum = checked_add_amount(*sum, line.allocated_amount)?;
        }
        for entry in entries {
            let id = entry.base.id.as_str();
            result.insert(
                entry,
                ReceiptEntryCapacity {
                    settled: settled.get(id).copied().unwrap_or_else(Amount::zero),
                    offset: reduced.get(id).copied().unwrap_or_else(Amount::zero),
                    reserved: pending.get(id).copied().unwrap_or_else(Amount::zero),
                    available: Amount::zero(),
                },
            )?;
        }
        Ok(result)
    }

    fn insert(&mut self, entry: &ReceivableEntry, mut capacity: ReceiptEntryCapacity) -> Result<()> {
        let account = self
            .accounts
            .get_mut(entry.receivable_account_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收子账不存在".into()))?;
        account.open = checked_sub_amount(account.open, capacity.reserved)?;
        if entry.direction == EntryDirection::Increase {
            capacity.available = checked_sub_amount(
                checked_sub_amount(checked_sub_amount(entry.amount, capacity.settled)?, capacity.offset)?,
                capacity.reserved,
            )?
            .max(Amount::zero());
        }
        self.entries.insert(entry.base.id.clone(), (account.id.clone(), capacity));
        Ok(())
    }

    /// 查询单个项目当前可分配额度，同时受子账可用余额限制。
    ///
    /// # 参数
    /// * `entry_id` - 应收项目身份。
    ///
    /// # 返回
    /// 返回可用额和组成金额；未知分录返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn entry(&self, entry_id: &str) -> Option<ReceiptEntryCapacity> {
        let (account_id, capacity) = self.entries.get(entry_id)?;
        let mut capacity = *capacity;
        capacity.available = capacity.available.min(self.accounts.get(account_id)?.open.max(Amount::zero()));
        Some(capacity)
    }

    /// 校验本次回款全部分配，不允许重复项目或跨主体，并合计检查子账余额。
    ///
    /// # 参数
    /// * `receipt` - 本次拟提交或最终核销的回款；额度已排除其自身占用。
    ///
    /// # 返回
    /// 每项及合计均在真实可用额度内时成功。
    ///
    /// # 错误
    /// 缺失来源、跨主体、重复项目、非正金额、超额或溢出时返回错误。
    pub fn validate(&self, receipt: &CustomerReceipt) -> Result<()> {
        let mut seen = HashSet::new();
        let mut totals = HashMap::new();
        for line in &receipt.pending_allocations {
            let id = line.receivable_entry_id.as_ref();
            if !seen.insert(id) || line.allocated_amount <= Amount::zero() {
                return Err(Error::ValidationError("核销项目不可重复且金额必须大于零".into()));
            }
            let (account_id, _) = self
                .entries
                .get(id)
                .ok_or_else(|| Error::NotFound("应收项目不存在，请刷新后重新选择".into()))?;
            let account = &self.accounts[account_id];
            if account.party_id != receipt.counterparty_party_id {
                return Err(Error::BusinessLogicError("禁止跨往来主体核销".into()));
            }
            let available = self.entry(id).expect("分录及其子账已在构造时校验").available;
            let total = totals.entry(account_id).or_insert(Amount::zero());
            *total = checked_add_amount(*total, line.allocated_amount)?;
            if line.allocated_amount > available || *total > account.open {
                return Err(Error::ConflictError(
                    "应收可核销余额不足（已扣除待审批回款占用），请刷新后调整核销金额".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{CustomerReceiptId, ReceivableAccountId, ReceivableEntryId};

    use super::*;
    use crate::entity::receivable::{CustomerReceiptData, ReceivableEntryType};

    fn entry(id: &str) -> ReceivableEntry {
        ReceivableEntry {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            receivable_account_id: ReceivableAccountId::new("account"),
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: "165".parse().unwrap(),
            due_date: BusinessDate::from_ymd(2026, 10, 11).unwrap(),
            source_fact_type: "sales_order".into(),
            source_document_id: "order".into(),
            source_revision_id: "revision".into(),
            source_sequence: 1,
            posted_at: Instant::from_unix_secs(1),
        }
    }

    fn line(id: &str, amount: &str) -> PendingReceiptAllocation {
        PendingReceiptAllocation {
            receivable_entry_id: ReceivableEntryId::new(id),
            allocated_amount: amount.parse().unwrap(),
        }
    }

    fn account(open: &str) -> ReceiptAccountCapacity {
        ReceiptAccountCapacity {
            id: "account".into(),
            party_id: PartyId::new("party"),
            open: open.parse().unwrap(),
        }
    }

    fn receipt(lines: Vec<PendingReceiptAllocation>) -> CustomerReceipt {
        let mut receipt = CustomerReceipt::new(
            CustomerReceiptId::new("receipt"),
            CustomerReceiptData {
                receipt_no: "SK-001".into(),
                counterparty_party_id: PartyId::new("party"),
                customer_id: None,
                received_at: Instant::from_unix_secs(1),
                amount: "1000".parse().unwrap(),
                bank_reference: None,
            },
            "creator",
        )
        .unwrap();
        receipt.start_approval(lines).unwrap();
        receipt
    }

    fn capacity(
        open: &str,
        entries: &[ReceivableEntry],
        reserved: &[PendingReceiptAllocation],
    ) -> ReceiptCapacity {
        ReceiptCapacity::new(vec![account(open)], entries, &[], &[], reserved).unwrap()
    }

    fn allocation(action: AllocationAction, amount: &str) -> ReceiptAllocation {
        ReceiptAllocation {
            base: BaseModel::fake(),
            customer_receipt_id: CustomerReceiptId::new("posted"),
            receivable_entry_id: ReceivableEntryId::new("entry"),
            allocation_seq: 1,
            allocation_action: action,
            allocated_amount: amount.parse().unwrap(),
            allocated_at: Instant::from_unix_secs(1),
            reverses_allocation_id: None,
        }
    }

    #[test]
    fn full_and_partial_pending_reservations_limit_new_receipts() {
        for (reserved, available) in [("165", "0"), ("100", "65")] {
            let snapshot = capacity("165", &[entry("entry")], &[line("entry", reserved)]);
            assert_eq!(snapshot.entry("entry").unwrap().available, available.parse().unwrap());
            assert_eq!(snapshot.entry("entry").unwrap().reserved, reserved.parse().unwrap());
            assert!(matches!(
                snapshot.validate(&receipt(vec![line("entry", "65.01")])),
                Err(Error::ConflictError(_))
            ));
        }
        capacity("165", &[entry("entry")], &[line("entry", "100")])
            .validate(&receipt(vec![line("entry", "65")]))
            .unwrap();
    }

    #[test]
    fn actual_settlements_reversals_offsets_and_pending_are_deducted_once() {
        let offsets = vec![ReceivableEntryOffset {
            base: BaseModel::fake(),
            decrease_entry_id: ReceivableEntryId::new("decrease"),
            increase_entry_id: ReceivableEntryId::new("entry"),
            offset_sequence: 1,
            offset_amount: "10.01".parse().unwrap(),
        }];
        let snapshot = ReceiptCapacity::new(
            vec![account("129.99")],
            &[entry("entry")],
            &[allocation(AllocationAction::Apply, "30"), allocation(AllocationAction::Reverse, "5")],
            &offsets,
            &[line("entry", "100")],
        )
        .unwrap();
        let result = snapshot.entry("entry").unwrap();
        assert_eq!(result.settled, "25".parse().unwrap());
        assert_eq!(result.offset, "10.01".parse().unwrap());
        assert_eq!(result.available, "29.99".parse().unwrap());
        snapshot.validate(&receipt(vec![line("entry", "29.99")])).unwrap();
    }

    #[test]
    fn sibling_reservations_limit_account_and_batch_total() {
        let snapshot = capacity("165", &[entry("first"), entry("second")], &[line("first", "100")]);
        assert_eq!(snapshot.entry("second").unwrap().available, "65".parse().unwrap());
        snapshot.validate(&receipt(vec![line("first", "30"), line("second", "35")])).unwrap();
        assert!(matches!(
            snapshot.validate(&receipt(vec![line("first", "40"), line("second", "40")])),
            Err(Error::ConflictError(_))
        ));
    }

    #[test]
    fn duplicate_nonpositive_missing_and_cross_party_are_rejected() {
        let snapshot = capacity("165", &[entry("entry")], &[]);
        assert!(matches!(
            snapshot.validate(&receipt(vec![line("entry", "20"), line("entry", "20")])),
            Err(Error::ValidationError(_))
        ));
        for amount in ["0", "-1"] {
            let mut invalid = receipt(vec![line("entry", "1")]);
            invalid.pending_allocations[0].allocated_amount = amount.parse().unwrap();
            assert!(matches!(snapshot.validate(&invalid), Err(Error::ValidationError(_))));
        }
        assert!(matches!(snapshot.validate(&receipt(vec![line("missing", "1")])), Err(Error::NotFound(_))));
        let mut other = receipt(vec![line("entry", "1")]);
        other.counterparty_party_id = PartyId::new("other");
        assert!(matches!(snapshot.validate(&other), Err(Error::BusinessLogicError(_))));
    }

    #[test]
    fn settled_overbooked_and_decrease_entries_have_zero_capacity() {
        for open in ["0", "-10"] {
            assert_eq!(
                capacity(open, &[entry("entry")], &[]).entry("entry").unwrap().available,
                Amount::zero()
            );
        }
        assert_eq!(
            capacity("165", &[entry("entry")], &[line("entry", "200")]).entry("entry").unwrap().available,
            Amount::zero()
        );
        let mut decrease = entry("decrease");
        decrease.direction = EntryDirection::Decrease;
        let snapshot = capacity("165", &[decrease], &[]);
        assert!(matches!(
            snapshot.validate(&receipt(vec![line("decrease", "1")])),
            Err(Error::ConflictError(_))
        ));
        assert!(snapshot.entry("unknown").is_none());
    }

    #[test]
    fn release_and_posting_preserve_available_balance() {
        // 读取已排除本单，最终核销不得再次扣除自身占用。
        capacity("165", &[entry("entry")], &[line("entry", "65")])
            .validate(&receipt(vec![line("entry", "100")]))
            .unwrap();
        let posted = ReceiptCapacity::new(
            vec![account("65")],
            &[entry("entry")],
            &[allocation(AllocationAction::Apply, "100")],
            &[],
            &[line("entry", "65")],
        )
        .unwrap();
        assert_eq!(posted.entry("entry").unwrap().available, Amount::zero());
        let released = ReceiptCapacity::new(
            vec![account("65")],
            &[entry("entry")],
            &[allocation(AllocationAction::Apply, "100")],
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(released.entry("entry").unwrap().available, "65".parse().unwrap());
    }

    #[test]
    fn overflow_returns_error_and_empty_scope_has_no_candidates() {
        assert!(
            ReceiptCapacity::new(
                vec![account("165")],
                &[entry("entry")],
                &[],
                &[],
                &[line("entry", "79228162514264337593543950335"), line("entry", "1")]
            )
            .is_err()
        );
        assert!(ReceiptCapacity::new(vec![], &[], &[], &[], &[]).unwrap().entry("entry").is_none());
    }
}
