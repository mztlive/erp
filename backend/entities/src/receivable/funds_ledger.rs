//! `ReceivableFundsLedger` 回款核销账本值对象（FIN-E13）。
//!
//! 以已装载分录事实一次完成 apply／reverse、可分配余额、连续序号与新
//! `ReceiptAllocation` 构造。分录／账户存在性、跨 party、事务与并发仍由
//! Service＋Repository 负责，ID 与核销时间由 Service 注入。

use std::collections::HashMap;

use super::funds_snapshot::{checked_add_amount, checked_sub_amount, net_receipt_allocated, zero_amount};
use super::{
    AllocationAction, PendingReceiptAllocation, ReceiptAllocation, ReceiptAllocationData, ReceivableEntry,
};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{CustomerReceiptId, ReceiptAllocationId, ReceivableAccountId};

/// 回款核销账本：按冻结待过账行完成净额、余额、序号与实体构造。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivableFundsLedger {
    receipt_id: CustomerReceiptId,
    net_allocated_total: AmountLike,
    pending: Vec<PendingReceiptAllocation>,
    pending_actions: Vec<AllocationAction>,
    pending_reverses: Vec<Option<ReceiptAllocationId>>,
    allocations: Vec<ReceiptAllocation>,
    account_deltas: Vec<(ReceivableAccountId, crate::money::Amount)>,
    account_delta_index: HashMap<String, usize>,
    entry_allocated: HashMap<String, crate::money::Amount>,
    seqs: Vec<u32>,
    applied_count: usize,
}

/// 账本内部使用的已验证合计别名，避免与实体字段混淆。
type AmountLike = crate::money::Amount;

impl ReceivableFundsLedger {
    /// 依据既有分配与待过账行构建回款核销账本。
    ///
    /// 完成既有净额（APPLY 加、REVERSE 减）、待过账合计与「净已核销不得超过
    /// 回款金额」上限校验，并为本次待过账行预分配连续序号。
    ///
    /// # 参数
    /// * `receipt_id` - 被核销回款单
    /// * `receipt_amount` - 回款含税金额（上限）
    /// * `existing` - 同一回款单已持久化核销分配
    /// * `pending` - 本次待过账行（顺序即序号顺序）
    ///
    /// # 返回
    /// 返回可逐行 [`Self::apply`] 的账本。
    ///
    /// # 错误
    /// 净额／合计溢出、合计超过回款金额或序号溢出时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 不生成 ID、不读写数据库、不校验分录／账户存在性。
    pub fn new(
        receipt_id: CustomerReceiptId,
        receipt_amount: crate::money::Amount,
        existing: &[ReceiptAllocation],
        pending: &[PendingReceiptAllocation],
    ) -> Result<Self> {
        Self::new_with_actions(
            receipt_id,
            receipt_amount,
            existing,
            pending,
            &vec![AllocationAction::Apply; pending.len()],
            &vec![None; pending.len()],
        )
    }

    /// 按显式 APPLY／REVERSE 动作构建账本，允许冲减既有占用。
    ///
    /// APPLY 行增加净额，REVERSE 行减少净额；净额必须落在 `[0, receipt_amount]`。
    ///
    /// # 参数
    /// * `receipt_id` - 被核销回款单
    /// * `receipt_amount` - 回款含税金额（上限）
    /// * `existing` - 同一回款单已持久化核销分配
    /// * `pending` - 本次待过账行
    /// * `actions` - 与 pending 等长的动作
    /// * `reverses` - REVERSE 行对应的原 APPLY；APPLY 行为 `None`
    ///
    /// # 返回
    /// 返回可按动作逐行 [`Self::apply`]／[`Self::reverse`] 的账本。
    ///
    /// # 错误
    /// 长度不一致、净额为负或超过回款、溢出或序号溢出时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 过账路径继续走 [`Self::new`]（全部 APPLY）；冲减既有占用必须走本入口。
    pub fn new_with_actions(
        receipt_id: CustomerReceiptId,
        receipt_amount: crate::money::Amount,
        existing: &[ReceiptAllocation],
        pending: &[PendingReceiptAllocation],
        actions: &[AllocationAction],
        reverses: &[Option<ReceiptAllocationId>],
    ) -> Result<Self> {
        if pending.len() != actions.len() || pending.len() != reverses.len() {
            return Err(Error::from("核销计划行数超过待过账行数"));
        }
        let existing_net = net_receipt_allocated(existing)?;
        let pending_net = pending.iter().zip(actions.iter()).try_fold(
            zero_amount(),
            |sum, (line, action)| match action {
                AllocationAction::Apply => checked_add_amount(sum, line.allocated_amount),
                AllocationAction::Reverse => checked_sub_amount(sum, line.allocated_amount),
            },
        )?;
        let net_allocated_total = checked_add_amount(existing_net, pending_net)?;
        if net_allocated_total.to_decimal().is_sign_negative() || net_allocated_total > receipt_amount {
            return Err(Error::from("核销合计超过回款金额"));
        }
        let mut entry_allocated: HashMap<String, crate::money::Amount> = HashMap::new();
        for allocation in existing {
            let balance = entry_allocated
                .entry(allocation.receivable_entry_id.to_string())
                .or_insert_with(zero_amount);
            let next = match allocation.allocation_action {
                AllocationAction::Apply => checked_add_amount(*balance, allocation.allocated_amount)?,
                AllocationAction::Reverse => checked_sub_amount(*balance, allocation.allocated_amount)?,
            };
            *balance = next;
        }
        let seqs = ReceiptAllocation::next_allocation_seq_range(existing, pending.len())?;
        Ok(Self {
            receipt_id,
            net_allocated_total,
            pending: pending.to_vec(),
            pending_actions: actions.to_vec(),
            pending_reverses: reverses.to_vec(),
            allocations: Vec::with_capacity(pending.len()),
            account_deltas: Vec::new(),
            account_delta_index: HashMap::new(),
            entry_allocated,
            seqs,
            applied_count: 0,
        })
    }

    /// 按历史登记计划构造待过账行并构建账本。
    ///
    /// # 参数
    /// * `receipt_id` - 新建历史回款
    /// * `receipt_amount` - 登记含税金额
    /// * `plan` - [`super::ReceivableFundsSnapshot::plan_historical_receipt_allocations`] 产出
    ///
    /// # 返回
    /// 返回待逐行 apply 的账本。
    ///
    /// # 错误
    /// 计划行为空或金额非正时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 历史路径与审批过账共享同一账本类型。
    pub fn from_historical_plan(
        receipt_id: CustomerReceiptId,
        receipt_amount: crate::money::Amount,
        plan: &[(crate::ids::ReceivableEntryId, crate::money::Amount)],
    ) -> Result<Self> {
        let pending = plan
            .iter()
            .map(|(entry_id, allocated)| PendingReceiptAllocation::new(entry_id.clone(), *allocated))
            .collect::<Result<Vec<_>>>()?;
        Self::new(receipt_id, receipt_amount, &[], &pending)
    }

    /// 按待过账行顺序核销一条分录。
    ///
    /// # 参数
    /// * `line` - 必须等于冻结的 `pending[applied_count]`
    /// * `entry` - 该行已装载分录
    /// * `allocation_id` - Service 注入的分配 ID
    /// * `allocated_at` - Service 注入的核销时间
    ///
    /// # 返回
    /// 本行写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 乱序、分录不一致、开放余额不足或溢出时返回 [`Error::LogicError`]；
    /// 失败保持调用前状态。
    ///
    /// # 约束
    /// 同分录多行共享开放余额；账户增量按首次出现顺序聚合。
    pub fn apply(
        &mut self,
        line: &PendingReceiptAllocation,
        entry: &ReceivableEntry,
        allocation_id: ReceiptAllocationId,
        allocated_at: Instant,
    ) -> Result<()> {
        self.apply_action(
            line,
            entry,
            allocation_id,
            allocated_at,
            AllocationAction::Apply,
            None,
        )
    }

    /// 按待过账行顺序冲减一条分录占用。
    ///
    /// # 参数
    /// * `line` - 必须等于冻结的 `pending[applied_count]`
    /// * `entry` - 该行已装载分录
    /// * `allocation_id` - Service 注入的分配 ID
    /// * `allocated_at` - Service 注入的核销时间
    /// * `reverses_allocation_id` - 被冲减的原 APPLY
    ///
    /// # 返回
    /// 本行写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 乱序、分录不一致、冲减超过已占用或溢出时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 冲减减少账户聚合增量；账户增量保持首次出现顺序。
    pub fn reverse(
        &mut self,
        line: &PendingReceiptAllocation,
        entry: &ReceivableEntry,
        allocation_id: ReceiptAllocationId,
        allocated_at: Instant,
        reverses_allocation_id: ReceiptAllocationId,
    ) -> Result<()> {
        self.apply_action(
            line,
            entry,
            allocation_id,
            allocated_at,
            AllocationAction::Reverse,
            Some(reverses_allocation_id),
        )
    }

    /// 返回本次构造的核销分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回按待过账行顺序排列的分配切片。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 供 Service 批量插入。
    pub fn new_allocations(&self) -> &[ReceiptAllocation] {
        &self.allocations
    }

    /// 返回按子账聚合的本次核销增量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `(子账, 正增量)`，供 `apply_settlements_many`。REVERSE 净额见
    /// [`Self::account_revert_deltas`]。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 过账路径只产出 APPLY 正增量。
    pub fn account_settlement_deltas(&self) -> Vec<(ReceivableAccountId, crate::money::Amount)> {
        self.account_deltas
            .iter()
            .filter(|(_, amount)| *amount > zero_amount())
            .cloned()
            .collect()
    }

    /// 返回按子账聚合的本次冲减增量（正数）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回既有占用被本批 REVERSE 冲减的 `(子账, 正数金额)`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 供 `revert_settlement`；过账 APPLY 路径为空。
    pub fn account_revert_deltas(&self) -> Vec<(ReceivableAccountId, crate::money::Amount)> {
        self.account_deltas
            .iter()
            .filter(|(_, amount)| amount.to_decimal().is_sign_negative())
            .map(|(id, amount)| {
                (
                    id.clone(),
                    crate::money::Amount::try_from(-amount.to_decimal()).unwrap_or(*amount),
                )
            })
            .collect()
    }

    /// 返回冻结的待过账行。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回构造时的 pending 切片。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// `apply` 必须按此顺序传入。
    pub fn pending(&self) -> &[PendingReceiptAllocation] {
        &self.pending
    }

    /// 返回净已核销合计。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回既有净额加本次待过账合计。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 构造时已保证不超过回款金额。
    pub fn net_allocated_total(&self) -> crate::money::Amount {
        self.net_allocated_total
    }

    /// 按动作写入一行并更新分录占用与账户增量。
    ///
    /// # 参数
    /// * `line` - 待过账行
    /// * `entry` - 分录事实
    /// * `allocation_id` - 分配 ID
    /// * `allocated_at` - 核销时间
    /// * `action` - APPLY 或 REVERSE
    /// * `reverses_allocation_id` - REVERSE 必填
    ///
    /// # 返回
    /// 成功写入返回 `Ok(())`。
    ///
    /// # 错误
    /// 顺序、余额或溢出失败时不写入本行。
    ///
    /// # 约束
    /// 账户增量按符号聚合：APPLY 为正，REVERSE 为负；既有占用冲减允许从零起记负数。
    fn apply_action(
        &mut self,
        line: &PendingReceiptAllocation,
        entry: &ReceivableEntry,
        allocation_id: ReceiptAllocationId,
        allocated_at: Instant,
        action: AllocationAction,
        reverses_allocation_id: Option<ReceiptAllocationId>,
    ) -> Result<()> {
        let seq = self.expected_seq(line)?;
        if self.pending_actions.get(self.applied_count).copied() != Some(action) {
            return Err(Error::from("核销行与待过账顺序不一致"));
        }
        if action == AllocationAction::Reverse
            && self.pending_reverses.get(self.applied_count).cloned().flatten() != reverses_allocation_id
        {
            return Err(Error::from("核销行与待过账顺序不一致"));
        }
        if entry.base.id != line.receivable_entry_id.to_string() {
            return Err(Error::from("核销分录事实与回款行不一致"));
        }
        let current = self
            .entry_allocated
            .get(entry.base.id.as_str())
            .copied()
            .unwrap_or_else(zero_amount);
        let next_balance = match action {
            AllocationAction::Apply => {
                let next = checked_add_amount(current, line.allocated_amount)?;
                if next > entry.amount {
                    return Err(Error::from("核销金额超过应收分录开放余额"));
                }
                next
            }
            AllocationAction::Reverse => {
                if line.allocated_amount > current {
                    return Err(Error::from("核销金额超过应收分录开放余额"));
                }
                checked_sub_amount(current, line.allocated_amount)?
            }
        };
        let allocation = ReceiptAllocation::new(
            allocation_id,
            ReceiptAllocationData {
                customer_receipt_id: self.receipt_id.clone(),
                receivable_entry_id: line.receivable_entry_id.clone(),
                allocation_seq: seq,
                allocation_action: action,
                allocated_amount: line.allocated_amount,
                allocated_at,
                reverses_allocation_id,
            },
        )?;
        let account_id = entry.receivable_account_id.clone();
        let account_index = self.account_delta_index.get(account_id.as_ref()).copied();
        let signed = match action {
            AllocationAction::Apply => line.allocated_amount,
            AllocationAction::Reverse => {
                // 账户进度以正增量 APPLY 推进；REVERSE 从已聚合增量扣减。
                line.allocated_amount
            }
        };
        let next_account_total = match (account_index, action) {
            (Some(index), AllocationAction::Apply) => {
                checked_add_amount(self.account_deltas[index].1, signed)?
            }
            (Some(index), AllocationAction::Reverse) => {
                checked_sub_amount(self.account_deltas[index].1, signed)?
            }
            (None, AllocationAction::Apply) => signed,
            (None, AllocationAction::Reverse) => checked_sub_amount(zero_amount(), signed)?,
        };
        self.entry_allocated.insert(entry.base.id.clone(), next_balance);
        match account_index {
            Some(index) => self.account_deltas[index].1 = next_account_total,
            None => {
                self.account_delta_index
                    .insert(account_id.to_string(), self.account_deltas.len());
                self.account_deltas.push((account_id, next_account_total));
            }
        }
        self.allocations.push(allocation);
        self.applied_count += 1;
        Ok(())
    }

    /// 校验传入行即冻结的下一待过账行并返回预分配序号。
    ///
    /// # 参数
    /// * `line` - 调用方传入行
    ///
    /// # 返回
    /// 顺序匹配时返回该行序号。
    ///
    /// # 错误
    /// 已用尽或乱序时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 不修改账本。
    fn expected_seq(&self, line: &PendingReceiptAllocation) -> Result<u32> {
        let expected = self
            .pending
            .get(self.applied_count)
            .ok_or_else(|| Error::from("核销计划行数超过待过账行数"))?;
        if expected != line {
            return Err(Error::from("核销行与待过账顺序不一致"));
        }
        Ok(self.seqs[self.applied_count])
    }
}

#[cfg(test)]
mod tests {
    use super::ReceivableFundsLedger;
    use crate::common::time::{BusinessDate, Instant};
    use crate::ids::{CustomerReceiptId, ReceiptAllocationId, ReceivableAccountId, ReceivableEntryId};
    use crate::money::Amount;
    use crate::receivable::{
        AllocationAction, EntryDirection, PendingReceiptAllocation, ReceiptAllocation, ReceiptAllocationData,
        ReceivableEntry, ReceivableEntryData, ReceivableEntryType,
    };
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn entry(id: &str, account: &str, value: &str) -> ReceivableEntry {
        ReceivableEntry::new(
            ReceivableEntryId::new(id),
            ReceivableEntryData {
                receivable_account_id: ReceivableAccountId::new(account),
                entry_type: ReceivableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: amount(value),
                due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
                source_fact_type: "sales_order".to_string(),
                source_document_id: "so-1".to_string(),
                source_revision_id: "sor-1".to_string(),
                source_sequence: 1,
                posted_at: Instant::from_unix_secs(1_700_000_000),
            },
        )
        .unwrap()
    }

    fn pending(entry_id: &str, value: &str) -> PendingReceiptAllocation {
        PendingReceiptAllocation::new(ReceivableEntryId::new(entry_id), amount(value)).unwrap()
    }

    fn existing(
        id: &str,
        entry_id: &str,
        seq: u32,
        action: AllocationAction,
        value: &str,
        reverses: Option<&str>,
    ) -> ReceiptAllocation {
        ReceiptAllocation::new(
            ReceiptAllocationId::new(id),
            ReceiptAllocationData {
                customer_receipt_id: CustomerReceiptId::new("cr-1"),
                receivable_entry_id: ReceivableEntryId::new(entry_id),
                allocation_seq: seq,
                allocation_action: action,
                allocated_amount: amount(value),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: reverses.map(ReceiptAllocationId::new),
            },
        )
        .unwrap()
    }

    #[test]
    fn apply_conserves_receipt_entry_and_account() {
        let pending_lines = vec![pending("e-1", "40.00"), pending("e-1", "10.00")];
        let mut ledger = ReceivableFundsLedger::new(
            CustomerReceiptId::new("cr-1"),
            amount("100.00"),
            &[],
            &pending_lines,
        )
        .unwrap();
        let entry = entry("e-1", "ra-1", "80.00");
        ledger
            .apply(
                &pending_lines[0],
                &entry,
                ReceiptAllocationId::new("al-1"),
                Instant::from_unix_secs(1),
            )
            .unwrap();
        ledger
            .apply(
                &pending_lines[1],
                &entry,
                ReceiptAllocationId::new("al-2"),
                Instant::from_unix_secs(1),
            )
            .unwrap();
        assert_eq!(ledger.net_allocated_total(), amount("50.00"));
        assert_eq!(ledger.new_allocations().len(), 2);
        assert_eq!(ledger.new_allocations()[0].allocation_seq, 1);
        assert_eq!(ledger.new_allocations()[1].allocation_seq, 2);
        assert_eq!(
            ledger.account_settlement_deltas(),
            vec![(ReceivableAccountId::new("ra-1"), amount("50.00"))]
        );
    }

    #[test]
    fn reverse_reduces_net_and_account_delta() {
        let existing_lines = vec![existing(
            "old-1",
            "e-1",
            1,
            AllocationAction::Apply,
            "50.00",
            None,
        )];
        let pending_lines = vec![pending("e-1", "20.00")];
        let mut ledger = ReceivableFundsLedger::new_with_actions(
            CustomerReceiptId::new("cr-1"),
            amount("100.00"),
            &existing_lines,
            &pending_lines,
            &[AllocationAction::Reverse],
            &[Some(ReceiptAllocationId::new("old-1"))],
        )
        .unwrap();
        let entry = entry("e-1", "ra-1", "80.00");
        ledger
            .reverse(
                &pending_lines[0],
                &entry,
                ReceiptAllocationId::new("al-r"),
                Instant::from_unix_secs(1),
                ReceiptAllocationId::new("old-1"),
            )
            .unwrap();
        assert_eq!(ledger.net_allocated_total(), amount("30.00"));
        assert!(ledger.account_settlement_deltas().is_empty());
        assert_eq!(
            ledger.account_revert_deltas(),
            vec![(ReceivableAccountId::new("ra-1"), amount("20.00"))]
        );
        assert_eq!(
            ledger.new_allocations()[0].allocation_action,
            AllocationAction::Reverse
        );
    }

    #[test]
    fn overflow_and_seq_fail_closed_without_partial_plan() {
        let huge = amount("999999999999999999999999999.00");
        let pending_lines = vec![pending("e-1", "999999999999999999999999999.00")];
        let existing_lines = vec![existing(
            "old-1",
            "e-1",
            1,
            AllocationAction::Apply,
            "999999999999999999999999999.00",
            None,
        )];
        let overflow = ReceivableFundsLedger::new(
            CustomerReceiptId::new("cr-1"),
            huge,
            &existing_lines,
            &pending_lines,
        );
        assert!(overflow.is_err(), "既有净额加待过账必须溢出失败");

        let mut max_seq = existing("old-max", "e-1", u32::MAX, AllocationAction::Apply, "10.00", None);
        max_seq.allocation_seq = u32::MAX;
        let seq_overflow = ReceivableFundsLedger::new(
            CustomerReceiptId::new("cr-1"),
            amount("100.00"),
            &[max_seq],
            &[pending("e-1", "1.00")],
        );
        assert!(seq_overflow.is_err(), "序号溢出不得产生部分计划");
    }

    #[test]
    fn reverse_after_apply_in_same_batch_conserves() {
        let pending_lines = vec![pending("e-1", "50.00"), pending("e-1", "20.00")];
        let mut ledger = ReceivableFundsLedger::new_with_actions(
            CustomerReceiptId::new("cr-1"),
            amount("100.00"),
            &[],
            &pending_lines,
            &[AllocationAction::Apply, AllocationAction::Reverse],
            &[None, Some(ReceiptAllocationId::new("al-1"))],
        )
        .unwrap();
        let entry = entry("e-1", "ra-1", "80.00");
        ledger
            .apply(
                &pending_lines[0],
                &entry,
                ReceiptAllocationId::new("al-1"),
                Instant::from_unix_secs(1),
            )
            .unwrap();
        ledger
            .reverse(
                &pending_lines[1],
                &entry,
                ReceiptAllocationId::new("al-2"),
                Instant::from_unix_secs(1),
                ReceiptAllocationId::new("al-1"),
            )
            .unwrap();
        assert_eq!(
            ledger.account_settlement_deltas(),
            vec![(ReceivableAccountId::new("ra-1"), amount("30.00"))]
        );
        assert_eq!(
            ledger.new_allocations()[1].allocation_action,
            AllocationAction::Reverse
        );
    }

    #[test]
    fn insufficient_entry_balance_and_receipt_cap_fail_closed() {
        let pending_lines = vec![pending("e-1", "90.00")];
        let mut ledger = ReceivableFundsLedger::new(
            CustomerReceiptId::new("cr-1"),
            amount("100.00"),
            &[],
            &pending_lines,
        )
        .unwrap();
        let entry = entry("e-1", "ra-1", "80.00");
        assert_eq!(
            ledger
                .apply(
                    &pending_lines[0],
                    &entry,
                    ReceiptAllocationId::new("al-1"),
                    Instant::from_unix_secs(1),
                )
                .unwrap_err()
                .to_string(),
            "核销金额超过应收分录开放余额"
        );
        assert!(ledger.new_allocations().is_empty());

        let over_receipt = vec![pending("e-1", "120.00")];
        assert_eq!(
            ReceivableFundsLedger::new(
                CustomerReceiptId::new("cr-1"),
                amount("100.00"),
                &[],
                &over_receipt,
            )
            .unwrap_err()
            .to_string(),
            "核销合计超过回款金额"
        );
    }

    #[test]
    fn out_of_order_and_sequence_are_deterministic() {
        let pending_lines = vec![pending("e-1", "10.00"), pending("e-2", "20.00")];
        let mut ledger = ReceivableFundsLedger::new(
            CustomerReceiptId::new("cr-1"),
            amount("100.00"),
            &[],
            &pending_lines,
        )
        .unwrap();
        let e2 = entry("e-2", "ra-1", "50.00");
        assert!(ledger
            .apply(
                &pending_lines[1],
                &e2,
                ReceiptAllocationId::new("al-1"),
                Instant::from_unix_secs(1),
            )
            .is_err());
        let e1 = entry("e-1", "ra-2", "50.00");
        ledger
            .apply(
                &pending_lines[0],
                &e1,
                ReceiptAllocationId::new("al-1"),
                Instant::from_unix_secs(1),
            )
            .unwrap();
        ledger
            .apply(
                &pending_lines[1],
                &e2,
                ReceiptAllocationId::new("al-2"),
                Instant::from_unix_secs(1),
            )
            .unwrap();
        assert_eq!(ledger.account_settlement_deltas().len(), 2);
        assert_eq!(ledger.new_allocations()[0].allocation_seq, 1);
        assert_eq!(ledger.new_allocations()[1].allocation_seq, 2);
    }

    #[test]
    fn historical_plan_builds_contiguous_sequences() {
        let plan = vec![
            (ReceivableEntryId::new("e-1"), amount("10.00")),
            (ReceivableEntryId::new("e-2"), amount("15.00")),
        ];
        let ledger = ReceivableFundsLedger::from_historical_plan(
            CustomerReceiptId::new("cr-1"),
            amount("25.00"),
            &plan,
        )
        .unwrap();
        assert_eq!(ledger.pending().len(), 2);
        assert_eq!(ledger.net_allocated_total(), amount("25.00"));
    }
}
