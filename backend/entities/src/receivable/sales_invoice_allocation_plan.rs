//! `SalesInvoiceAllocationPlan` 销项发票分配计划值对象（FIN-E08）。
//!
//! 与 [`SalesInvoiceAllocation`](crate::receivable::SalesInvoiceAllocation) 实体的
//! 分工：本计划负责发票总额／税额三口径（gross / net / tax）与发票应收额精确
//! 守恒、输入顺序确定的连续序号、分配实体生成与按子账聚合的开票增量归位；
//! `SalesInvoiceAllocation::new` 继续守卫单行自身的金额正数与
//! `gross = net + tax` 恒等不变量。计划不生成 ID、不读取数据库——发票 ID 与
//! 分配行 ID 由 Service 注入；账户存在性、跨主体一致性、可开票额度、
//! 事务与并发仍由 Service＋Repository 负责。

use std::collections::HashMap;
use std::str::FromStr;

use crate::errors::{Error, Result};
use crate::ids::{InvoiceId, ReceivableAccountId, SalesInvoiceAllocationId};
use crate::money::Amount;
use crate::receivable::{AllocationAction, SalesInvoiceAllocation, SalesInvoiceAllocationData};

/// 销项发票分配计划输入行（金额三元组由计划统一校验）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesInvoiceAllocationLine {
    /// 销售单可开票对象（应收往来子账）。
    pub receivable_account_id: ReceivableAccountId,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 分配税额。
    pub allocated_tax_amount: Amount,
}

/// 销项发票分配计划（FIN-E08）。
///
/// 以发票应收额（gross / net / tax）与输入行一次完成全部校验并生成不可部分
/// 成功的计划：
/// - 至少一条分配；每行金额为正且满足 `gross = net + tax`（实体不变量）；
/// - 三口径合计与发票应收额精确守恒（少分、超分、税额口径漂移均拒绝）；
/// - 序号按输入顺序从 1 连续分配；相同输入必然产出相同计划（确定性）；
/// - 按子账聚合开票增量（首次出现顺序），同一账户只推进一次聚合值。
///
/// 任一步校验失败即整体失败，不产生部分计划；金额运算全部为精确定点运算，
/// 任一步溢出即失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesInvoiceAllocationPlan {
    /// 本次构造的销项发票分配（输入顺序，序号从 1 连续）。
    allocations: Vec<SalesInvoiceAllocation>,
    /// 按子账聚合的本次开票增量（首次出现顺序，确定性）。
    account_deltas: Vec<(ReceivableAccountId, Amount)>,
}

impl SalesInvoiceAllocationPlan {
    /// 依据发票应收额与输入行构建销项发票分配计划。
    ///
    /// 集中完成总额／税额口径守恒校验（三口径与发票应收额精确相等）、序号
    /// 分配、分配实体生成与按子账聚合开票增量；任一输入行金额非正或恒等
    /// 不成立时由实体构造器拒绝，整体不产生部分计划。
    ///
    /// # 参数
    /// * `invoice_id` - 目标销项发票
    /// * `invoice_gross` - 发票含税应收额（守恒基准）
    /// * `invoice_net` - 发票不含税应收额（守恒基准）
    /// * `invoice_tax` - 发票税额（守恒基准）
    /// * `lines` - 分配输入行（顺序即序号与实体顺序）
    /// * `allocation_ids` - 每行注入的分配实体 ID（长度必须与 `lines` 一致）
    ///
    /// # 返回
    /// 返回校验通过、可整体持久化的计划。
    ///
    /// # 错误
    /// 行数为空、ID 数量与行数不一致、金额合计溢出、三口径任一与发票不守恒、
    /// 任一行金额非正或恒等不成立时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 纯内存确定性构造：不生成 ID、不读写数据库、不校验账户存在性
    /// 与可开票额度；存在性与额度由 Service 装载事实后校验，并发安全由
    /// Service 的事务与 Repository 条件更新保证。
    pub fn new(
        invoice_id: InvoiceId,
        invoice_gross: Amount,
        invoice_net: Amount,
        invoice_tax: Amount,
        lines: &[SalesInvoiceAllocationLine],
        allocation_ids: &[SalesInvoiceAllocationId],
    ) -> Result<Self> {
        if lines.is_empty() {
            return Err(Error::from("至少提供一条发票分配"));
        }
        if lines.len() != allocation_ids.len() {
            return Err(Error::from("分配 ID 数量必须与分配行数一致"));
        }
        let gross_total = lines.iter().try_fold(zero_amount(), |sum, line| {
            checked_add_amount(sum, line.allocated_gross_amount)
        })?;
        let net_total = lines.iter().try_fold(zero_amount(), |sum, line| {
            checked_add_amount(sum, line.allocated_net_amount)
        })?;
        let tax_total = lines.iter().try_fold(zero_amount(), |sum, line| {
            checked_add_amount(sum, line.allocated_tax_amount)
        })?;
        if gross_total != invoice_gross {
            return Err(Error::from("发票分配合计必须等于发票金额"));
        }
        if net_total != invoice_net {
            return Err(Error::from("发票分配不含税合计必须等于发票不含税金额"));
        }
        if tax_total != invoice_tax {
            return Err(Error::from("发票分配税额合计必须等于发票税额"));
        }
        let mut allocations = Vec::with_capacity(lines.len());
        let mut account_deltas: Vec<(ReceivableAccountId, Amount)> = Vec::new();
        let mut account_delta_index: HashMap<String, usize> = HashMap::new();
        for (index, line) in lines.iter().enumerate() {
            let seq = u32::try_from(index + 1).map_err(|_| Error::from("分配行数超过序号上限"))?;
            allocations.push(SalesInvoiceAllocation::new(
                allocation_ids[index].clone(),
                SalesInvoiceAllocationData {
                    invoice_id: invoice_id.clone(),
                    receivable_account_id: line.receivable_account_id.clone(),
                    allocation_seq: seq,
                    allocation_action: AllocationAction::Apply,
                    allocated_gross_amount: line.allocated_gross_amount,
                    allocated_net_amount: line.allocated_net_amount,
                    allocated_tax_amount: line.allocated_tax_amount,
                    reverses_allocation_id: None,
                },
            )?);
            match account_delta_index.get(line.receivable_account_id.as_ref()) {
                Some(&delta_index) => {
                    let (_, total) = &mut account_deltas[delta_index];
                    *total = checked_add_amount(*total, line.allocated_gross_amount)?;
                }
                None => {
                    account_delta_index.insert(line.receivable_account_id.to_string(), account_deltas.len());
                    account_deltas.push((line.receivable_account_id.clone(), line.allocated_gross_amount));
                }
            }
        }
        Ok(Self {
            allocations,
            account_deltas,
        })
    }

    /// 返回本次构造的销项发票分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回按输入顺序排列、序号从 1 连续的分配切片，供 Service 批量持久化。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 调用方不能修改计划；事务与持久化职责仍属于 Service。
    pub fn new_allocations(&self) -> &[SalesInvoiceAllocation] {
        &self.allocations
    }

    /// 返回按子账聚合的本次开票增量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `(子账, 增量金额)` 列表，按输入行首次出现顺序排列（确定性），
    /// 同一账户只出现一次且金额为全部行含税金额之和，供 Service 按子账执行
    /// 并发安全的条件开票。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 聚合只保证金额守恒；子账存在性、跨主体一致性与可开票额度由
    /// Service 校验。
    pub fn account_invoicing_deltas(&self) -> &[(ReceivableAccountId, Amount)] {
        &self.account_deltas
    }
}

/// 返回固定零金额。
///
/// # 返回
/// 返回金额 `0.00`。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
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
        .ok_or_else(|| Error::from("发票分配金额合计溢出"))?;
    Amount::try_from(sum).map_err(|_| Error::from("发票分配金额合计溢出"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn line(account: &str, gross: &str, net: &str, tax: &str) -> SalesInvoiceAllocationLine {
        SalesInvoiceAllocationLine {
            receivable_account_id: ReceivableAccountId::new(account),
            allocated_gross_amount: Amount::from_str(gross).unwrap(),
            allocated_net_amount: Amount::from_str(net).unwrap(),
            allocated_tax_amount: Amount::from_str(tax).unwrap(),
        }
    }

    fn ids(count: usize) -> Vec<SalesInvoiceAllocationId> {
        (0..count)
            .map(|index| SalesInvoiceAllocationId::new(format!("alloc-{index}")))
            .collect()
    }

    fn build(
        invoice_gross: &str,
        invoice_net: &str,
        invoice_tax: &str,
        lines: &[SalesInvoiceAllocationLine],
    ) -> Result<SalesInvoiceAllocationPlan> {
        SalesInvoiceAllocationPlan::new(
            InvoiceId::new("invoice-1"),
            Amount::from_str(invoice_gross).unwrap(),
            Amount::from_str(invoice_net).unwrap(),
            Amount::from_str(invoice_tax).unwrap(),
            lines,
            &ids(lines.len()),
        )
    }

    #[test]
    fn under_allocation_rejected() {
        let lines = [line("acct-1", "90.00", "90.00", "0.00")];
        let err = build("100.00", "100.00", "0.00", &lines).unwrap_err();
        assert!(err.to_string().contains("发票分配合计必须等于发票金额"));
    }

    #[test]
    fn over_allocation_rejected() {
        let lines = [line("acct-1", "110.00", "110.00", "0.00")];
        let err = build("100.00", "100.00", "0.00", &lines).unwrap_err();
        assert!(err.to_string().contains("发票分配合计必须等于发票金额"));
    }

    #[test]
    fn net_calibre_mismatch_rejected() {
        let lines = [line("acct-1", "100.00", "90.00", "10.00")];
        let err = build("100.00", "94.00", "6.00", &lines).unwrap_err();
        assert!(
            err.to_string()
                .contains("发票分配不含税合计必须等于发票不含税金额"),
            "税额口径漂移必须被计划拒绝，实际错误：{}",
            err
        );
    }

    #[test]
    fn tax_calibre_mismatch_rejected() {
        let lines = [line("acct-1", "100.00", "94.00", "6.00")];
        let err = build("100.00", "94.00", "7.00", &lines).unwrap_err();
        assert!(err.to_string().contains("发票分配税额合计必须等于发票税额"));
    }

    #[test]
    fn line_amount_identity_rejected() {
        let lines = [
            line("acct-1", "60.00", "50.00", "5.00"),
            line("acct-2", "40.00", "44.00", "1.00"),
        ];
        let err = build("100.00", "94.00", "6.00", &lines).unwrap_err();
        assert!(err.to_string().contains("分配含税金额必须等于不含税金额加税额"));
    }

    #[test]
    fn zero_and_negative_amounts_rejected() {
        let zero = [line("acct-1", "0.00", "0.00", "0.00")];
        assert!(build("0.00", "0.00", "0.00", &zero).is_err(), "零金额必须拒绝");
        let negative = [line("acct-1", "-1.00", "-1.00", "0.00")];
        assert!(
            build("-1.00", "-1.00", "0.00", &negative).is_err(),
            "负金额必须拒绝"
        );
    }

    #[test]
    fn sequences_consecutive_from_one_in_input_order() {
        let lines = [
            line("acct-1", "50.00", "47.00", "3.00"),
            line("acct-2", "30.00", "30.00", "0.00"),
            line("acct-1", "20.00", "17.00", "3.00"),
        ];
        let plan = build("100.00", "94.00", "6.00", &lines).unwrap();
        let seqs: Vec<u32> = plan.new_allocations().iter().map(|a| a.allocation_seq).collect();
        assert_eq!(seqs, vec![1, 2, 3]);
        assert_eq!(
            plan.new_allocations()[0].allocated_gross_amount,
            Amount::from_str("50.00").unwrap()
        );
        assert_eq!(
            plan.new_allocations()[2].allocated_gross_amount,
            Amount::from_str("20.00").unwrap()
        );
    }

    #[test]
    fn same_account_lines_aggregate_once_with_conservation() {
        let lines = [
            line("acct-1", "50.00", "47.00", "3.00"),
            line("acct-2", "30.00", "30.00", "0.00"),
            line("acct-1", "20.00", "17.00", "3.00"),
        ];
        let plan = build("100.00", "94.00", "6.00", &lines).unwrap();
        assert_eq!(
            plan.account_invoicing_deltas(),
            &[
                (
                    ReceivableAccountId::new("acct-1"),
                    Amount::from_str("70.00").unwrap()
                ),
                (
                    ReceivableAccountId::new("acct-2"),
                    Amount::from_str("30.00").unwrap()
                ),
            ],
            "同一账户只推进聚合值且按首次出现顺序"
        );
        let gross: Amount = plan
            .new_allocations()
            .iter()
            .fold(zero_amount(), |sum, a| sum.checked_add(a.allocated_gross_amount));
        let net: Amount = plan
            .new_allocations()
            .iter()
            .fold(zero_amount(), |sum, a| sum.checked_add(a.allocated_net_amount));
        let tax: Amount = plan
            .new_allocations()
            .iter()
            .fold(zero_amount(), |sum, a| sum.checked_add(a.allocated_tax_amount));
        assert_eq!(gross, Amount::from_str("100.00").unwrap());
        assert_eq!(net, Amount::from_str("94.00").unwrap());
        assert_eq!(tax, Amount::from_str("6.00").unwrap());
    }

    #[test]
    fn input_order_determinism() {
        let lines = [
            line("acct-1", "60.00", "56.40", "3.60"),
            line("acct-2", "40.00", "37.60", "2.40"),
        ];
        let first = build("100.00", "94.00", "6.00", &lines).unwrap();
        let second = build("100.00", "94.00", "6.00", &lines).unwrap();
        assert_eq!(first, second, "相同输入必须产出相同计划");
        let swapped_lines = [lines[1].clone(), lines[0].clone()];
        let swapped = build("100.00", "94.00", "6.00", &swapped_lines).unwrap();
        assert_eq!(swapped.new_allocations()[0].allocation_seq, 1);
        assert_eq!(
            swapped.new_allocations()[0].allocated_gross_amount,
            Amount::from_str("40.00").unwrap()
        );
        assert_ne!(first, swapped);
    }

    #[test]
    fn empty_lines_and_id_mismatch_rejected() {
        assert!(build("100.00", "94.00", "6.00", &[]).is_err(), "空行必须拒绝");
        let lines = [line("acct-1", "100.00", "94.00", "6.00")];
        let err = SalesInvoiceAllocationPlan::new(
            InvoiceId::new("invoice-1"),
            Amount::from_str("100.00").unwrap(),
            Amount::from_str("94.00").unwrap(),
            Amount::from_str("6.00").unwrap(),
            &lines,
            &[],
        )
        .unwrap_err();
        assert!(err.to_string().contains("分配 ID 数量必须与分配行数一致"));
    }

    #[test]
    fn arithmetic_overflow_rejected() {
        let max = Amount::try_from(Decimal::MAX).unwrap();
        let lines = [
            line("acct-1", "1.00", "1.00", "0.00"),
            line("acct-2", "1.00", "1.00", "0.00"),
        ];
        let mut overflow = vec![lines[0].clone(), lines[1].clone()];
        overflow[0].allocated_gross_amount = max;
        overflow[1].allocated_gross_amount = max;
        let err = build("2.00", "2.00", "0.00", &overflow).unwrap_err();
        assert!(
            err.to_string().contains("溢出"),
            "合计溢出必须失败，实际错误：{}",
            err
        );
    }
}
