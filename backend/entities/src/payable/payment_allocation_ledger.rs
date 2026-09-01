//! `PaymentAllocationLedger` 付款核销账本值对象（FIN-E02）。
//!
//! 与 [`PaymentAllocation`](crate::payable::PaymentAllocation) 实体的分工：
//! 本账本负责既有分配净额（`APPLY` 加、`REVERSE` 减）、逐分录开放余额、
//! 连续序号与核销分配实体构造的确定性归位；`PaymentAllocation::new` 继续
//! 守卫单行自身的金额正数、序号起点与「动作 ↔ 原分配引用」不变量。
//! 账本不生成 ID、不读取数据库、不校验供应商一致性——分录／供应商存在性、
//! 跨供应商一致性、事务与并发仍由 Service＋Repository 负责，ID 与核销时间
//! 由 Service 注入。

use std::collections::HashMap;
use std::str::FromStr;

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{PayableAccountId, PaymentAllocationId, SupplierPaymentId};
use crate::money::Amount;
use crate::payable::{
    AllocationAction, PayableEntry, PaymentAllocation, PaymentAllocationData, PendingPaymentAllocation,
};

/// 付款核销账本：以已装载分录事实逐行完成净额、余额、序号与实体构造。
///
/// 构造规则（与 `PayableService::post_supplier_payment_in_transaction`
/// 原实现一致，责任归位后 Service 不再手工求和）：
/// - `new` 先按既有分配计算净已核销合计并与待过账合计比对付款金额上限；
/// - `apply` 按待过账行顺序逐行校验分录开放余额、分配连续序号并构造
///   `PaymentAllocation`；
/// - 同分录多行共享开放余额进度，重复分录/子账只聚合一次，不重复推进；
/// - 全部金额运算为精确定点运算，任一步溢出即失败，不产生部分计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentAllocationLedger {
    /// 付款单。
    payment_id: SupplierPaymentId,
    /// 净已核销合计（既有净额 + 本次待过账合计）。
    net_allocated_total: Amount,
    /// 本次构造的核销分配（顺序与待过账行一致）。
    allocations: Vec<PaymentAllocation>,
    /// 按子账聚合的本次核销增量（首次出现顺序，确定性）。
    account_deltas: Vec<(PayableAccountId, Amount)>,
    /// 子账增量索引（`account_deltas` 内位置）。
    account_delta_index: HashMap<String, usize>,
    /// 逐分录已占用核销额（既有净额 + 本次已过账行）。
    entry_allocated: HashMap<String, Amount>,
    /// 本次待过账行分配的连续序号。
    seqs: Vec<u32>,
    /// 已过账行数。
    applied_count: usize,
}

impl PaymentAllocationLedger {
    /// 依据既有分配与待过账行构建付款核销账本。
    ///
    /// 完成既有净额计算（`APPLY` 加、`REVERSE` 减）、待过账合计与
    /// 「净已核销 + 本次合计不得超过付款金额」上限校验，并为本次待过账行
    /// 预分配连续序号（`max(allocation_seq) + 1` 起）。
    ///
    /// # 参数
    /// * `payment_id` - 被核销付款单
    /// * `payment_amount` - 付款单含税金额（上限）
    /// * `existing` - 同一付款单已持久化核销分配
    /// * `pending` - 本次待过账核销行（顺序即序号顺序）
    ///
    /// # 返回
    /// 返回校验通过、可逐行 [`Self::apply`] 的账本。
    ///
    /// # 错误
    /// 净额/合计运算溢出、合计超过付款金额或序号区间溢出时返回
    /// [`Error::LogicError`]，且不产生部分计划。
    ///
    /// # 约束
    /// 不生成 ID、不读写数据库、不校验分录/供应商存在性；分录事实由
    /// Service 装载后逐行传入 [`Self::apply`]。
    pub fn new(
        payment_id: SupplierPaymentId,
        payment_amount: Amount,
        existing: &[PaymentAllocation],
        pending: &[PendingPaymentAllocation],
    ) -> Result<Self> {
        let existing_net = existing.iter().try_fold(zero_amount(), |sum, line| {
            let delta = match line.allocation_action {
                AllocationAction::Apply => line.allocated_amount,
                AllocationAction::Reverse => negate_amount(line.allocated_amount)?,
            };
            checked_add_amount(sum, delta)
        })?;
        let pending_total = pending.iter().try_fold(zero_amount(), |sum, line| {
            checked_add_amount(sum, line.allocated_amount)
        })?;
        let net_allocated_total = checked_add_amount(existing_net, pending_total)?;
        if net_allocated_total > payment_amount {
            return Err(Error::from("核销合计超过付款金额"));
        }
        let mut entry_allocated: HashMap<String, Amount> = HashMap::new();
        for allocation in existing {
            let balance = entry_allocated
                .entry(allocation.payable_entry_id.to_string())
                .or_insert_with(zero_amount);
            let delta = match allocation.allocation_action {
                AllocationAction::Apply => allocation.allocated_amount,
                AllocationAction::Reverse => negate_amount(allocation.allocated_amount)?,
            };
            *balance = checked_add_amount(*balance, delta)?;
        }
        let seqs = PaymentAllocation::next_allocation_seq_range(existing, pending.len())?;
        Ok(Self {
            payment_id,
            net_allocated_total,
            allocations: Vec::with_capacity(pending.len()),
            account_deltas: Vec::new(),
            account_delta_index: HashMap::new(),
            entry_allocated,
            seqs,
            applied_count: 0,
        })
    }

    /// 按待过账行顺序逐行核销一条分录。
    ///
    /// 校验分录事实与行身份一致、开放余额充足（既有占用 + 本次已过账行 +
    /// 本行不超过分录含税金额），随后构造核销分配并按分录所属子账聚合增量。
    /// 调用方必须按 `pending` 顺序逐行调用，且先完成分录/子账存在性与跨
    /// 供应商一致性校验。
    ///
    /// # 参数
    /// * `line` - 待过账核销行
    /// * `entry` - 该行已装载的应付分录事实
    /// * `allocation_id` - 本行核销分配 ID（由 Service 生成后注入）
    /// * `allocated_at` - 核销发生时间（由 Service 注入）
    ///
    /// # 返回
    /// 本行核销构造成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 分录事实与行不一致、超出待过账行数、开放余额不足或金额运算溢出时
    /// 返回 [`Error::LogicError`]；账本保持失败前状态，不产生部分计划。
    ///
    /// # 约束
    /// 纯内存计算，不依赖数据库自然顺序；并发正确性由 Service 的条件更新
    /// 与事务保证。
    pub fn apply(
        &mut self,
        line: &PendingPaymentAllocation,
        entry: &PayableEntry,
        allocation_id: PaymentAllocationId,
        allocated_at: Instant,
    ) -> Result<()> {
        if entry.base.id != line.payable_entry_id.to_string() {
            return Err(Error::from("核销分录事实与付款行不一致"));
        }
        let seq = *self
            .seqs
            .get(self.applied_count)
            .ok_or_else(|| Error::from("核销计划行数超过待过账行数"))?;
        let balance = self
            .entry_allocated
            .entry(entry.base.id.clone())
            .or_insert_with(zero_amount);
        let next_balance = checked_add_amount(*balance, line.allocated_amount)?;
        if next_balance > entry.amount {
            return Err(Error::from("核销金额超过应付分录开放余额"));
        }
        let allocation = PaymentAllocation::new(
            allocation_id,
            PaymentAllocationData {
                supplier_payment_id: self.payment_id.clone(),
                payable_entry_id: line.payable_entry_id.clone(),
                allocation_seq: seq,
                allocation_action: AllocationAction::Apply,
                allocated_amount: line.allocated_amount,
                allocated_at,
                reverses_allocation_id: None,
            },
        )?;
        *balance = next_balance;
        match self.account_delta_index.get(entry.payable_account_id.as_ref()) {
            Some(&index) => {
                let (_, total) = &mut self.account_deltas[index];
                *total = checked_add_amount(*total, line.allocated_amount)?;
            }
            None => {
                self.account_delta_index
                    .insert(entry.payable_account_id.to_string(), self.account_deltas.len());
                self.account_deltas
                    .push((entry.payable_account_id.clone(), line.allocated_amount));
            }
        }
        self.allocations.push(allocation);
        self.applied_count += 1;
        Ok(())
    }

    /// 返回本次构造的核销分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回按待过账行顺序排列的分配切片，供 Service 批量持久化。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 调用方不能修改账本；事务与持久化职责仍属于 Service。
    pub fn new_allocations(&self) -> &[PaymentAllocation] {
        &self.allocations
    }

    /// 返回按子账聚合的本次核销增量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `(子账, 增量金额)` 列表，按待过账行首次出现顺序排列（确定性），
    /// 供 Service 按子账执行并发安全的条件核销。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 聚合只保证金额守恒；子账存在性与供应商一致性由 Service 校验。
    pub fn account_settlement_deltas(&self) -> &[(PayableAccountId, Amount)] {
        &self.account_deltas
    }

    /// 返回净已核销合计（既有净额 + 本次待过账合计）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `APPLY` 加、`REVERSE` 减后的净核销总额。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 只反映构造时的事实快照；付款金额守恒由 [`Self::new`] 保证。
    pub fn net_allocated_total(&self) -> Amount {
        self.net_allocated_total
    }
}

/// 返回固定零金额。
///
/// # 返回
/// 返回金额 `0.00`。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

/// 返回金额的相反数（仅用于 `REVERSE` 方向的净额扣减）。
///
/// # 参数
/// * `amount` - 正数金额
///
/// # 返回
/// 返回符号相反且数值相等的金额。
///
/// # 错误
/// 取反溢出时返回 [`Error::LogicError`]。
fn negate_amount(amount: Amount) -> Result<Amount> {
    let negated = -amount.to_decimal();
    Amount::try_from(negated).map_err(|_| Error::from("核销金额合计溢出"))
}

/// 精确相加两个金额（溢出时失败）。
///
/// # 参数
/// * `left` - 加数
/// * `right` - 加数
///
/// # 返回
/// 返回精确和。
///
/// # 错误
/// 定点运算溢出时返回 [`Error::LogicError`]。
fn checked_add_amount(left: Amount, right: Amount) -> Result<Amount> {
    let sum = left
        .to_decimal()
        .checked_add(right.to_decimal())
        .ok_or_else(|| Error::from("核销金额合计溢出"))?;
    Amount::try_from(sum).map_err(|_| Error::from("核销金额合计溢出"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PayableEntryId;
    use rust_decimal::Decimal;

    fn entry(id: &str, account: &str, amount: &str) -> PayableEntry {
        PayableEntry::new(
            PayableEntryId::new(id),
            crate::payable::PayableEntryData {
                payable_account_id: PayableAccountId::new(account),
                entry_type: crate::payable::PayableEntryType::Original,
                direction: crate::payable::EntryDirection::Increase,
                amount: Amount::from_str(amount).unwrap(),
                due_date: crate::common::time::BusinessDate::from_ymd(2026, 9, 30).unwrap(),
                source_fact_type: "purchase_order".to_string(),
                source_document_id: "PO-1".to_string(),
                source_revision_id: "rev-1".to_string(),
                source_sequence: 1,
                posted_at: Instant::from_unix_secs(1_700_000_000),
            },
        )
        .unwrap()
    }

    fn pending_line(entry_id: &str, amount: &str) -> PendingPaymentAllocation {
        PendingPaymentAllocation::new(PayableEntryId::new(entry_id), Amount::from_str(amount).unwrap())
            .unwrap()
    }

    fn allocation(
        id: &str,
        entry_id: &str,
        seq: u32,
        action: AllocationAction,
        amount: &str,
        reverses: Option<&str>,
    ) -> PaymentAllocation {
        PaymentAllocation::new(
            PaymentAllocationId::new(id),
            PaymentAllocationData {
                supplier_payment_id: SupplierPaymentId::new("sp-1"),
                payable_entry_id: PayableEntryId::new(entry_id),
                allocation_seq: seq,
                allocation_action: action,
                allocated_amount: Amount::from_str(amount).unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: reverses.map(PaymentAllocationId::new),
            },
        )
        .unwrap()
    }

    fn build_ledger(
        payment_amount: &str,
        existing: &[PaymentAllocation],
        pending: &[PendingPaymentAllocation],
    ) -> PaymentAllocationLedger {
        PaymentAllocationLedger::new(
            SupplierPaymentId::new("sp-1"),
            Amount::from_str(payment_amount).unwrap(),
            existing,
            pending,
        )
        .unwrap()
    }

    #[test]
    fn forward_apply_keeps_three_way_conservation() {
        let e1 = entry("pe-1", "acct-1", "1000.00");
        let pending = [pending_line("pe-1", "300.00"), pending_line("pe-1", "200.00")];
        let mut ledger = build_ledger("500.00", &[], &pending);
        ledger
            .apply(
                &pending[0],
                &e1,
                PaymentAllocationId::new("pa-1"),
                Instant::from_unix_secs(1),
            )
            .unwrap();
        ledger
            .apply(
                &pending[1],
                &e1,
                PaymentAllocationId::new("pa-2"),
                Instant::from_unix_secs(2),
            )
            .unwrap();

        assert_eq!(ledger.net_allocated_total(), Amount::from_str("500.00").unwrap());
        let allocations = ledger.new_allocations();
        assert_eq!(allocations.len(), 2);
        assert_eq!(allocations[0].allocation_seq, 1);
        assert_eq!(allocations[1].allocation_seq, 2);
        assert_eq!(
            allocations[0].allocated_amount,
            Amount::from_str("300.00").unwrap()
        );
        assert_eq!(
            allocations[1].allocated_amount,
            Amount::from_str("200.00").unwrap()
        );
        assert_eq!(allocations[0].payable_entry_id, PayableEntryId::new("pe-1"));
        assert_eq!(allocations[0].allocation_action, AllocationAction::Apply);
        assert!(allocations[0].reverses_allocation_id.is_none());
        // 子账增量只聚合一次且金额守恒
        let deltas = ledger.account_settlement_deltas();
        assert_eq!(
            deltas,
            &[(
                PayableAccountId::new("acct-1"),
                Amount::from_str("500.00").unwrap()
            )]
        );
        // 三方守恒：付款净额 = 既有净额 + 新分配合计；分录余额 = 分配净额
        let new_total = allocations
            .iter()
            .fold(zero_amount(), |sum, a| sum.checked_add(a.allocated_amount));
        assert_eq!(ledger.net_allocated_total(), new_total);
        assert_eq!(new_total, Amount::from_str("500.00").unwrap());
    }

    #[test]
    fn existing_reverse_reduces_net_and_entry_balance() {
        let e1 = entry("pe-1", "acct-1", "1000.00");
        let existing = [
            allocation("pa-a1", "pe-1", 1, AllocationAction::Apply, "1000.00", None),
            allocation(
                "pa-r1",
                "pe-1",
                2,
                AllocationAction::Reverse,
                "300.00",
                Some("pa-a1"),
            ),
        ];
        let pending = [pending_line("pe-1", "200.00")];
        let mut ledger = build_ledger("1100.00", &existing, &pending);
        ledger
            .apply(
                &pending[0],
                &e1,
                PaymentAllocationId::new("pa-2"),
                Instant::from_unix_secs(1),
            )
            .unwrap();

        // 净额：1000 - 300 + 200 = 900；序号从既有最大值 2 后连续
        assert_eq!(ledger.net_allocated_total(), Amount::from_str("900.00").unwrap());
        let allocations = ledger.new_allocations();
        assert_eq!(allocations.len(), 1);
        assert_eq!(allocations[0].allocation_seq, 3);
        assert_eq!(
            allocations[0].allocated_amount,
            Amount::from_str("200.00").unwrap()
        );
    }

    #[test]
    fn pending_total_over_payment_amount_rejected() {
        let existing = [allocation(
            "pa-a1",
            "pe-1",
            1,
            AllocationAction::Apply,
            "400.00",
            None,
        )];
        let pending = [pending_line("pe-1", "500.00")];
        let err = PaymentAllocationLedger::new(
            SupplierPaymentId::new("sp-1"),
            Amount::from_str("800.00").unwrap(),
            &existing,
            &pending,
        )
        .unwrap_err();
        assert!(err.to_string().contains("核销合计超过付款金额"));
    }

    #[test]
    fn entry_open_balance_exceeded_rejected_without_partial_plan() {
        let e1 = entry("pe-1", "acct-1", "1000.00");
        let existing = [allocation(
            "pa-a1",
            "pe-1",
            1,
            AllocationAction::Apply,
            "900.00",
            None,
        )];
        let pending = [pending_line("pe-1", "200.00")];
        let mut ledger = build_ledger("1100.00", &existing, &pending);
        let err = ledger
            .apply(
                &pending[0],
                &e1,
                PaymentAllocationId::new("pa-2"),
                Instant::from_unix_secs(1),
            )
            .unwrap_err();
        assert!(err.to_string().contains("核销金额超过应付分录开放余额"));
        // 失败不产生部分计划
        assert!(ledger.new_allocations().is_empty());
        assert!(ledger.account_settlement_deltas().is_empty());
    }

    #[test]
    fn same_entry_multiple_lines_share_open_balance() {
        let e1 = entry("pe-1", "acct-1", "1000.00");
        let pending = [
            pending_line("pe-1", "600.00"),
            pending_line("pe-1", "400.00"),
            pending_line("pe-1", "0.01"),
        ];
        let mut ledger = build_ledger("1000.01", &[], &pending);
        ledger
            .apply(
                &pending[0],
                &e1,
                PaymentAllocationId::new("pa-1"),
                Instant::from_unix_secs(1),
            )
            .unwrap();
        ledger
            .apply(
                &pending[1],
                &e1,
                PaymentAllocationId::new("pa-2"),
                Instant::from_unix_secs(2),
            )
            .unwrap();
        // 同分录两行合计恰为开放余额，通过；第三行必然超过开放余额
        let err = ledger
            .apply(
                &pending[2],
                &e1,
                PaymentAllocationId::new("pa-3"),
                Instant::from_unix_secs(3),
            )
            .unwrap_err();
        assert!(err.to_string().contains("开放余额"));
        assert_eq!(ledger.new_allocations().len(), 2);
    }

    #[test]
    fn sequences_are_consecutive_after_existing_max() {
        let existing = [
            allocation("pa-a1", "pe-1", 5, AllocationAction::Apply, "10.00", None),
            allocation("pa-a2", "pe-2", 2, AllocationAction::Apply, "10.00", None),
        ];
        let e1 = entry("pe-3", "acct-1", "1000.00");
        let pending = [
            pending_line("pe-3", "10.00"),
            pending_line("pe-3", "10.00"),
            pending_line("pe-3", "10.00"),
        ];
        let mut ledger = build_ledger("1000.00", &existing, &pending);
        for (index, line) in pending.iter().enumerate() {
            ledger
                .apply(
                    line,
                    &e1,
                    PaymentAllocationId::new(format!("pa-n{index}")),
                    Instant::from_unix_secs(index as i64),
                )
                .unwrap();
        }
        let seqs: Vec<u32> = ledger
            .new_allocations()
            .iter()
            .map(|a| a.allocation_seq)
            .collect();
        assert_eq!(seqs, vec![6, 7, 8]);
    }

    #[test]
    fn order_determinism_and_apply_beyond_pending_rejected() {
        let e1 = entry("pe-1", "acct-1", "1000.00");
        let pending = [pending_line("pe-1", "100.00"), pending_line("pe-1", "200.00")];
        let mut first = build_ledger("1000.00", &[], &pending);
        for (index, line) in pending.iter().enumerate() {
            first
                .apply(
                    line,
                    &e1,
                    PaymentAllocationId::new(format!("pa-{index}")),
                    Instant::from_unix_secs(index as i64),
                )
                .unwrap();
        }
        let mut second = build_ledger("1000.00", &[], &pending);
        for (index, line) in pending.iter().enumerate() {
            second
                .apply(
                    line,
                    &e1,
                    PaymentAllocationId::new(format!("pa-{index}")),
                    Instant::from_unix_secs(index as i64),
                )
                .unwrap();
        }
        // 相同输入产出相同计划
        assert_eq!(first, second);
        // 行序决定序号与金额顺序
        assert_eq!(
            first.new_allocations()[0].allocated_amount,
            Amount::from_str("100.00").unwrap()
        );
        assert_eq!(first.new_allocations()[0].allocation_seq, 1);
        assert_eq!(
            first.new_allocations()[1].allocated_amount,
            Amount::from_str("200.00").unwrap()
        );
        assert_eq!(first.new_allocations()[1].allocation_seq, 2);
        // 超出待过账行数的 apply 被拒绝
        let err = first
            .apply(
                &pending_line("pe-1", "1.00"),
                &e1,
                PaymentAllocationId::new("pa-x"),
                Instant::from_unix_secs(9),
            )
            .unwrap_err();
        assert!(err.to_string().contains("超过待过账行数"));
    }

    #[test]
    fn mismatched_entry_fact_rejected() {
        let other = entry("pe-2", "acct-1", "1000.00");
        let pending = [pending_line("pe-1", "100.00")];
        let mut ledger = build_ledger("1000.00", &[], &pending);
        let err = ledger
            .apply(
                &pending[0],
                &other,
                PaymentAllocationId::new("pa-1"),
                Instant::from_unix_secs(1),
            )
            .unwrap_err();
        assert!(err.to_string().contains("核销分录事实与付款行不一致"));
    }

    #[test]
    fn multiple_accounts_aggregate_in_first_occurrence_order() {
        let e1 = entry("pe-1", "acct-1", "1000.00");
        let e2 = entry("pe-2", "acct-2", "1000.00");
        let pending = [
            pending_line("pe-1", "100.00"),
            pending_line("pe-2", "200.00"),
            pending_line("pe-1", "50.00"),
        ];
        let mut ledger = build_ledger("1000.00", &[], &pending);
        for (index, line) in pending.iter().enumerate() {
            let entry = if line.payable_entry_id.to_string() == "pe-1" {
                &e1
            } else {
                &e2
            };
            ledger
                .apply(
                    line,
                    entry,
                    PaymentAllocationId::new(format!("pa-{index}")),
                    Instant::from_unix_secs(index as i64),
                )
                .unwrap();
        }
        assert_eq!(
            ledger.account_settlement_deltas(),
            &[
                (
                    PayableAccountId::new("acct-1"),
                    Amount::from_str("150.00").unwrap()
                ),
                (
                    PayableAccountId::new("acct-2"),
                    Amount::from_str("200.00").unwrap()
                ),
            ]
        );
    }

    #[test]
    fn arithmetic_overflow_rejected() {
        let max = Amount::try_from(Decimal::MAX).unwrap();
        // 既有净额溢出
        let mut overflow_existing = vec![
            allocation("pa-a1", "pe-1", 1, AllocationAction::Apply, "1.00", None),
            allocation("pa-a2", "pe-2", 2, AllocationAction::Apply, "1.00", None),
        ];
        overflow_existing[0].allocated_amount = max;
        overflow_existing[1].allocated_amount = max;
        let err = PaymentAllocationLedger::new(SupplierPaymentId::new("sp-1"), max, &overflow_existing, &[])
            .unwrap_err();
        assert!(err.to_string().contains("溢出"));
        // 待过账合计溢出
        let mut overflow_pending = vec![pending_line("pe-1", "1.00"), pending_line("pe-2", "1.00")];
        overflow_pending[0].allocated_amount = max;
        overflow_pending[1].allocated_amount = max;
        let err = PaymentAllocationLedger::new(
            SupplierPaymentId::new("sp-1"),
            max,
            &[allocation(
                "pa-a1",
                "pe-1",
                1,
                AllocationAction::Apply,
                "1.00",
                None,
            )],
            &overflow_pending,
        )
        .unwrap_err();
        assert!(err.to_string().contains("溢出"));
    }

    #[test]
    fn empty_pending_builds_empty_plan() {
        let ledger = build_ledger("1000.00", &[], &[]);
        assert!(ledger.new_allocations().is_empty());
        assert!(ledger.account_settlement_deltas().is_empty());
        assert_eq!(ledger.net_allocated_total(), zero_amount());
    }
}
