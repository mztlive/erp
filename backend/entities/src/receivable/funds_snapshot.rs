//! `ReceivableFundsSnapshot` 卡券票款正式事实快照（FIN-E13）。
//!
//! 统一分录／回款／发票净额对账、summary 一致性、conclusion 前置条件、
//! 历史回款可分配计划与确定性 fact version。事实装载、party 跨聚合判断、
//! 事务及持久化仍由 Service／Repository 负责。

use std::collections::HashSet;
use std::str::FromStr;

use sha2::{Digest, Sha256};

use super::card_funds_review_decision::{
    CardFundsReviewConclusion, CardFundsReviewResult, CardFundsReviewType,
};
use super::{
    AllocationAction, CustomerReceipt, EntryDirection, Invoice, ReceiptAllocation, ReceivableAccount,
    ReceivableEntry, SalesInvoiceAllocation,
};
use crate::errors::{Error, Result};
use crate::ids::ReceivableEntryId;
use crate::money::Amount;

const FACT_HASH_PREFIX: &str = "receivable-funds-facts-v1";

/// 已装载且无重复主键的票款正式事实快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivableFundsSnapshot {
    entries: Vec<ReceivableEntry>,
    receipt_allocations: Vec<ReceiptAllocation>,
    invoice_allocations: Vec<SalesInvoiceAllocation>,
    receipts: Vec<CustomerReceipt>,
    invoices: Vec<Invoice>,
}

impl ReceivableFundsSnapshot {
    /// 从已装载事实构造快照并拒绝重复主键。
    ///
    /// # 参数
    /// * `entries` - 账户分录
    /// * `receipt_allocations` - 回款核销分配
    /// * `invoice_allocations` - 销项发票分配
    /// * `receipts` - 回款单
    /// * `invoices` - 发票
    ///
    /// # 返回
    /// 返回无重复 entry／receipt／invoice 主键的快照。
    ///
    /// # 错误
    /// 分录、回款或发票主键重复时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 不读取数据库，不对账账户汇总；对账见 [`Self::reconcile`]。
    pub fn from_facts(
        entries: Vec<ReceivableEntry>,
        receipt_allocations: Vec<ReceiptAllocation>,
        invoice_allocations: Vec<SalesInvoiceAllocation>,
        receipts: Vec<CustomerReceipt>,
        invoices: Vec<Invoice>,
    ) -> Result<Self> {
        ensure_unique_ids(entries.iter().map(|entry| entry.base.id.as_str()), "应收分录")?;
        ensure_unique_ids(receipts.iter().map(|receipt| receipt.base.id.as_str()), "回款单")?;
        ensure_unique_ids(invoices.iter().map(|invoice| invoice.base.id.as_str()), "发票")?;
        Ok(Self {
            entries,
            receipt_allocations,
            invoice_allocations,
            receipts,
            invoices,
        })
    }

    /// 返回分录事实。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回装载时的分录切片。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 不排序；调用方需要顺序时显式排序。
    pub fn entries(&self) -> &[ReceivableEntry] {
        &self.entries
    }

    /// 返回回款核销分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回装载时的分配切片。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 分配允许指向同一分录或同一回款。
    pub fn receipt_allocations(&self) -> &[ReceiptAllocation] {
        &self.receipt_allocations
    }

    /// 返回销项发票分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回装载时的分配切片。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 分配允许指向同一发票或同一账户。
    pub fn invoice_allocations(&self) -> &[SalesInvoiceAllocation] {
        &self.invoice_allocations
    }

    /// 返回回款单事实。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回装载时的回款切片。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 主键唯一已在构造时校验。
    pub fn receipts(&self) -> &[CustomerReceipt] {
        &self.receipts
    }

    /// 返回发票事实。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回装载时的发票切片。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 主键唯一已在构造时校验。
    pub fn invoices(&self) -> &[Invoice] {
        &self.invoices
    }

    /// 复算分录／回款／发票净额并与账户汇总对账。
    ///
    /// # 参数
    /// * `account` - 当前应收账户
    ///
    /// # 返回
    /// 三口径均守恒时返回 `Ok(())`。
    ///
    /// # 错误
    /// 分录净额、回款分配净额或发票分配净额与账户汇总不一致，或合计溢出时返回
    /// [`Error::LogicError`]，文案与原 Service 一致。
    ///
    /// # 约束
    /// 不修改账户；party 一致性仍由 Service 判断。
    pub fn reconcile(&self, account: &ReceivableAccount) -> Result<()> {
        if self.entries_net()? != account.gross_total {
            return Err(Error::from("应收分录净额与账户应收总额不一致"));
        }
        if self.receipt_net()? != account.settled_total {
            return Err(Error::from("回款分配净额与账户已核销总额不一致"));
        }
        if self.invoice_net()? != account.invoiced_total {
            return Err(Error::from("发票分配净额与账户已开票总额不一致"));
        }
        Ok(())
    }

    /// 校验正式结论的事实前提。
    ///
    /// # 参数
    /// * `review_type` - 期初或差额
    /// * `review_result` - 通过或驳回
    /// * `conclusion` - 从零／已核对／驳回
    ///
    /// # 返回
    /// 前置条件满足时返回 `Ok(())`。
    ///
    /// # 错误
    /// 从零起算或已核对结论与事实不符时返回原业务文案。
    ///
    /// # 约束
    /// 须在 [`Self::reconcile`] 之后调用；驳回结论无额外事实前提。
    pub fn validate_conclusion(
        &self,
        review_type: CardFundsReviewType,
        review_result: CardFundsReviewResult,
        conclusion: CardFundsReviewConclusion,
    ) -> Result<()> {
        let receipt_net = self.receipt_net()?;
        let invoice_net = self.invoice_net()?;
        let zero = zero_amount();
        match conclusion {
            CardFundsReviewConclusion::NoHistoryFromZero => {
                if review_type != CardFundsReviewType::Opening
                    || review_result != CardFundsReviewResult::Approved
                    || receipt_net != zero
                    || invoice_net != zero
                    || !self.receipt_allocations.is_empty()
                    || !self.invoice_allocations.is_empty()
                {
                    return Err(Error::from(
                        "从零起算只允许无任何历史回款和发票事实的期初通过复核",
                    ));
                }
            }
            CardFundsReviewConclusion::RecordedFactsReconciled => {
                if review_result != CardFundsReviewResult::Approved
                    || (self.receipt_allocations.is_empty() && self.invoice_allocations.is_empty())
                {
                    return Err(Error::from("已核对结论必须存在正式回款或发票事实"));
                }
            }
            CardFundsReviewConclusion::Rejected => {}
        }
        Ok(())
    }

    /// 计算账户及其当前票款正式事实的不透明版本。
    ///
    /// # 参数
    /// * `account` - 当前应收账户
    ///
    /// # 返回
    /// 返回 `ffv:` 前缀的确定性 hex。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 字段集合、排序键与长度前缀编码必须与原 `funds_fact_version` 字节级一致。
    pub fn fact_version(&self, account: &ReceivableAccount) -> String {
        let mut digest = Sha256::new();
        digest_part(&mut digest, FACT_HASH_PREFIX);
        for value in [
            account.base.id.as_str(),
            &account.base.version.to_string(),
            &account.account_seq.to_string(),
            account.counterparty_party_id.as_ref(),
            &account.gross_total.to_string(),
            &account.settled_total.to_string(),
            &account.invoiceable_total.to_string(),
            &account.invoiced_total.to_string(),
        ] {
            digest_part(&mut digest, value);
        }
        let mut entries = self.entries.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.source_sequence
                .cmp(&right.source_sequence)
                .then_with(|| left.base.id.cmp(&right.base.id))
        });
        for entry in entries {
            for value in [
                entry.base.id.as_str(),
                entry.entry_type.as_str(),
                entry.direction.as_str(),
                &entry.amount.to_string(),
                &entry.due_date.to_string(),
                entry.source_document_id.as_str(),
                entry.source_revision_id.as_str(),
                &entry.source_sequence.to_string(),
                &entry.posted_at.unix_secs().to_string(),
            ] {
                digest_part(&mut digest, value);
            }
        }
        let mut receipt_allocations = self.receipt_allocations.iter().collect::<Vec<_>>();
        receipt_allocations.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        for allocation in receipt_allocations {
            for value in [
                allocation.base.id.as_str(),
                allocation.customer_receipt_id.as_ref(),
                allocation.receivable_entry_id.as_ref(),
                &allocation.allocation_seq.to_string(),
                allocation.allocation_action.as_str(),
                &allocation.allocated_amount.to_string(),
                allocation
                    .reverses_allocation_id
                    .as_ref()
                    .map(AsRef::as_ref)
                    .unwrap_or_default(),
            ] {
                digest_part(&mut digest, value);
            }
        }
        let mut invoice_allocations = self.invoice_allocations.iter().collect::<Vec<_>>();
        invoice_allocations.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        for allocation in invoice_allocations {
            for value in [
                allocation.base.id.as_str(),
                allocation.invoice_id.as_ref(),
                allocation.receivable_account_id.as_ref(),
                &allocation.allocation_seq.to_string(),
                allocation.allocation_action.as_str(),
                &allocation.allocated_gross_amount.to_string(),
                &allocation.allocated_net_amount.to_string(),
                &allocation.allocated_tax_amount.to_string(),
                allocation
                    .reverses_allocation_id
                    .as_ref()
                    .map(AsRef::as_ref)
                    .unwrap_or_default(),
            ] {
                digest_part(&mut digest, value);
            }
        }
        let mut receipts = self.receipts.iter().collect::<Vec<_>>();
        receipts.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        for receipt in receipts {
            for value in [
                receipt.base.id.as_str(),
                &receipt.base.version.to_string(),
                receipt.status.as_str(),
                receipt.counterparty_party_id.as_ref(),
                receipt.receipt_no.as_str(),
                &receipt.received_at.unix_secs().to_string(),
                &receipt.amount.to_string(),
                receipt.bank_reference.as_deref().unwrap_or_default(),
            ] {
                digest_part(&mut digest, value);
            }
        }
        let mut invoices = self.invoices.iter().collect::<Vec<_>>();
        invoices.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        for invoice in invoices {
            for value in [
                invoice.base.id.as_str(),
                &invoice.base.version.to_string(),
                invoice.invoice_direction.as_str(),
                invoice.invoice_kind.as_str(),
                invoice.party_id.as_ref(),
                invoice.invoice_code.as_deref().unwrap_or_default(),
                invoice.invoice_no.as_str(),
                &invoice.invoice_date.to_string(),
                &invoice.gross_amount.to_string(),
                &invoice.net_amount.to_string(),
                &invoice.tax_amount.to_string(),
                invoice.stable.status().as_str(),
            ] {
                digest_part(&mut digest, value);
            }
        }
        format!("ffv:{}", hex::encode(digest.finalize()))
    }

    /// 按应收分录顺序为历史回款生成服务端核销计划。
    ///
    /// 只消费 `Increase` 分录，按 `source_sequence`、主键排序，扣减既有 APPLY／REVERSE
    /// 净占用后分配可分配余额，直到登记金额耗尽。
    ///
    /// # 参数
    /// * `amount` - 本次历史回款含税金额
    ///
    /// # 返回
    /// 返回保持分录顺序的 `(分录, 金额)` 计划。
    ///
    /// # 错误
    /// 金额非正、历史净占用超过分录金额或开放余额不足时返回原服务文案。
    ///
    /// # 约束
    /// 不写入分配实体；ID 与时间由 Service 注入账本。
    pub fn plan_historical_receipt_allocations(
        &self,
        amount: Amount,
    ) -> Result<Vec<(ReceivableEntryId, Amount)>> {
        if amount <= zero_amount() {
            return Err(Error::from("回款金额必须大于零"));
        }
        let mut entries = self
            .entries
            .iter()
            .filter(|entry| entry.direction == EntryDirection::Increase)
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.source_sequence
                .cmp(&right.source_sequence)
                .then_with(|| left.base.id.cmp(&right.base.id))
        });
        let mut unallocated = amount;
        let mut plan = Vec::new();
        for entry in entries {
            if unallocated == zero_amount() {
                break;
            }
            let allocated = self.entry_receipt_allocated(&entry.base.id)?;
            if allocated > entry.amount {
                return Err(Error::from("应收分录历史核销累计超过分录金额"));
            }
            let available = checked_sub_amount(entry.amount, allocated)?;
            if available == zero_amount() {
                continue;
            }
            let planned = if available <= unallocated {
                available
            } else {
                unallocated
            };
            plan.push((ReceivableEntryId::new(entry.base.id.clone()), planned));
            unallocated = checked_sub_amount(unallocated, planned)?;
        }
        if unallocated != zero_amount() {
            return Err(Error::from("应收分录开放余额不足，无法完成历史回款分配"));
        }
        Ok(plan)
    }

    /// 返回分录净额（Increase 加、Decrease 减）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与账户 `gross_total` 对账的净额。
    ///
    /// # 错误
    /// 合计溢出时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 不按账户过滤；调用方保证事实属于同一账户。
    pub fn entries_net(&self) -> Result<Amount> {
        self.entries
            .iter()
            .try_fold(zero_amount(), |total, entry| match entry.direction {
                EntryDirection::Increase => checked_add_amount(total, entry.amount),
                EntryDirection::Decrease => checked_sub_amount(total, entry.amount),
            })
    }

    /// 返回回款分配净额（APPLY 加、REVERSE 减）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与账户 `settled_total` 对账的净额。
    ///
    /// # 错误
    /// 合计溢出时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 四方守恒的回款口径。
    pub fn receipt_net(&self) -> Result<Amount> {
        net_receipt_allocated(&self.receipt_allocations)
    }

    /// 返回销项发票分配净额（APPLY 加、REVERSE 减）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与账户 `invoiced_total` 对账的净额。
    ///
    /// # 错误
    /// 合计溢出时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 四方守恒的发票口径。
    pub fn invoice_net(&self) -> Result<Amount> {
        self.invoice_allocations
            .iter()
            .try_fold(zero_amount(), |sum, line| match line.allocation_action {
                AllocationAction::Apply => checked_add_amount(sum, line.allocated_gross_amount),
                AllocationAction::Reverse => checked_sub_amount(sum, line.allocated_gross_amount),
            })
    }

    /// 返回指定分录的回款净占用。
    ///
    /// # 参数
    /// * `entry_id` - 分录主键
    ///
    /// # 返回
    /// 返回 APPLY 减 REVERSE 后的占用。
    ///
    /// # 错误
    /// 合计溢出时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 只聚合本快照内分配。
    fn entry_receipt_allocated(&self, entry_id: &str) -> Result<Amount> {
        self.receipt_allocations
            .iter()
            .filter(|line| line.receivable_entry_id.as_ref() == entry_id)
            .try_fold(zero_amount(), |total, line| match line.allocation_action {
                AllocationAction::Apply => checked_add_amount(total, line.allocated_amount),
                AllocationAction::Reverse => checked_sub_amount(total, line.allocated_amount),
            })
    }
}

/// 计算回款分配净已核销合计。
///
/// # 参数
/// * `allocations` - 回款核销分配
///
/// # 返回
/// 返回 APPLY 加、REVERSE 减后的净额。
///
/// # 错误
/// 合计溢出时返回 [`Error::LogicError`]。
///
/// # 约束
/// 供账本与快照共用，避免两套净额算法。
pub(crate) fn net_receipt_allocated(allocations: &[ReceiptAllocation]) -> Result<Amount> {
    allocations
        .iter()
        .try_fold(zero_amount(), |sum, line| match line.allocation_action {
            AllocationAction::Apply => checked_add_amount(sum, line.allocated_amount),
            AllocationAction::Reverse => checked_sub_amount(sum, line.allocated_amount),
        })
}

/// 校验主键集合无重复。
///
/// # 参数
/// * `ids` - 主键迭代
/// * `label` - 用于错误文案的对象名
///
/// # 返回
/// 无重复时返回 `Ok(())`。
///
/// # 错误
/// 发现重复时返回 [`Error::LogicError`]。
///
/// # 约束
/// 不排序输入。
fn ensure_unique_ids<'a>(ids: impl Iterator<Item = &'a str>, label: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(Error::from(format!("{label}事实主键重复")));
        }
    }
    Ok(())
}

/// 返回固定零金额。
///
/// # 返回
/// 返回 `0.00`。
pub(crate) fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

/// 精确相加两个金额。
///
/// # 参数
/// * `left` - 加数
/// * `right` - 加数
///
/// # 返回
/// 返回精确和。
///
/// # 错误
/// 溢出时返回 [`Error::LogicError`]。
pub(crate) fn checked_add_amount(left: Amount, right: Amount) -> Result<Amount> {
    let sum = left
        .to_decimal()
        .checked_add(right.to_decimal())
        .ok_or_else(|| Error::from("票款金额合计溢出"))?;
    Amount::try_from(sum).map_err(|_| Error::from("票款金额合计溢出"))
}

/// 精确相减两个金额。
///
/// # 参数
/// * `left` - 被减数
/// * `right` - 减数
///
/// # 返回
/// 返回精确差。
///
/// # 错误
/// 溢出时返回 [`Error::LogicError`]。
pub(crate) fn checked_sub_amount(left: Amount, right: Amount) -> Result<Amount> {
    let diff = left
        .to_decimal()
        .checked_sub(right.to_decimal())
        .ok_or_else(|| Error::from("票款金额合计溢出"))?;
    Amount::try_from(diff).map_err(|_| Error::from("票款金额合计溢出"))
}

/// 向摘要写入无拼接歧义的长度前缀字段。
///
/// # 参数
/// * `digest` - SHA-256 累加器
/// * `value` - 字段文本
///
/// # 返回
/// 无。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 与原 Service `digest_part` 编码一致。
fn digest_part(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::{
        CardFundsReviewConclusion, CardFundsReviewResult, CardFundsReviewType, ReceivableFundsSnapshot,
    };
    use crate::common::time::{BusinessDate, Instant};
    use crate::ids::{
        CustomerAccountId, CustomerReceiptId, InvoiceId, PartyId, ReceiptAllocationId, ReceivableAccountId,
        ReceivableEntryId, SalesInvoiceAllocationId, SalesOrderId, SalesOrderRevisionId,
    };
    use crate::money::Amount;
    use crate::receivable::{
        AccountReviewStatus, AllocationAction, CustomerReceipt, CustomerReceiptData, EntryDirection, Invoice,
        InvoiceData, InvoiceDirection, InvoiceKind, ReceiptAllocation, ReceiptAllocationData,
        ReceivableAccount, ReceivableAccountData, ReceivableEntry, ReceivableEntryData, ReceivableEntryType,
        SalesInvoiceAllocation, SalesInvoiceAllocationData,
    };
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn account(gross: &str, settled: &str, invoiced: &str) -> ReceivableAccount {
        ReceivableAccount::new(
            ReceivableAccountId::new("ra-1"),
            ReceivableAccountData {
                sales_order_id: SalesOrderId::new("so-1"),
                account_seq: 1,
                customer_id: CustomerAccountId::new("cust-1"),
                counterparty_party_id: PartyId::new("party-1"),
                source_sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
                review_status: AccountReviewStatus::OpeningPending,
                reviewed_by: None,
                reviewed_at: None,
                review_evidence_reference: None,
                gross_total: amount(gross),
                settled_total: amount(settled),
                invoiceable_total: amount(gross),
                invoiced_total: amount(invoiced),
            },
            "tester",
        )
        .unwrap()
    }

    fn entry(id: &str, seq: u32, value: &str) -> ReceivableEntry {
        ReceivableEntry::new(
            ReceivableEntryId::new(id),
            ReceivableEntryData {
                receivable_account_id: ReceivableAccountId::new("ra-1"),
                entry_type: ReceivableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: amount(value),
                due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
                source_fact_type: "sales_order".to_string(),
                source_document_id: "so-1".to_string(),
                source_revision_id: "sor-1".to_string(),
                source_sequence: seq,
                posted_at: Instant::from_unix_secs(1_700_000_000),
            },
        )
        .unwrap()
    }

    fn receipt_alloc(
        id: &str,
        entry_id: &str,
        seq: u32,
        value: &str,
        action: AllocationAction,
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
                reverses_allocation_id: match action {
                    AllocationAction::Apply => None,
                    AllocationAction::Reverse => Some(ReceiptAllocationId::new("ra-orig")),
                },
            },
        )
        .unwrap()
    }

    fn invoice_alloc(id: &str, seq: u32, gross: &str, action: AllocationAction) -> SalesInvoiceAllocation {
        let net = amount(gross);
        SalesInvoiceAllocation::new(
            SalesInvoiceAllocationId::new(id),
            SalesInvoiceAllocationData {
                invoice_id: InvoiceId::new("inv-1"),
                receivable_account_id: ReceivableAccountId::new("ra-1"),
                allocation_seq: seq,
                allocation_action: action,
                allocated_gross_amount: net,
                allocated_net_amount: net,
                allocated_tax_amount: amount("0.00"),
                reverses_allocation_id: match action {
                    AllocationAction::Apply => None,
                    AllocationAction::Reverse => Some(SalesInvoiceAllocationId::new("sia-orig")),
                },
            },
        )
        .unwrap()
    }

    fn empty_snapshot() -> ReceivableFundsSnapshot {
        ReceivableFundsSnapshot::from_facts(Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new())
            .unwrap()
    }

    #[test]
    fn zero_facts_reconcile_and_allow_opening_from_zero() {
        let snapshot = empty_snapshot();
        snapshot.reconcile(&account("0.00", "0.00", "0.00")).unwrap();
        snapshot
            .validate_conclusion(
                CardFundsReviewType::Opening,
                CardFundsReviewResult::Approved,
                CardFundsReviewConclusion::NoHistoryFromZero,
            )
            .unwrap();
        assert!(snapshot
            .validate_conclusion(
                CardFundsReviewType::Opening,
                CardFundsReviewResult::Approved,
                CardFundsReviewConclusion::RecordedFactsReconciled,
            )
            .is_err());
    }

    #[test]
    fn recorded_facts_require_allocations_and_reject_summary_drift() {
        let snapshot = ReceivableFundsSnapshot::from_facts(
            vec![entry("e-1", 1, "100.00")],
            vec![receipt_alloc("a-1", "e-1", 1, "40.00", AllocationAction::Apply)],
            vec![invoice_alloc("i-1", 1, "60.00", AllocationAction::Apply)],
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        snapshot.reconcile(&account("100.00", "40.00", "60.00")).unwrap();
        snapshot
            .validate_conclusion(
                CardFundsReviewType::Opening,
                CardFundsReviewResult::Approved,
                CardFundsReviewConclusion::RecordedFactsReconciled,
            )
            .unwrap();
        assert_eq!(
            snapshot
                .reconcile(&account("100.00", "10.00", "60.00"))
                .unwrap_err()
                .to_string(),
            "回款分配净额与账户已核销总额不一致"
        );
        assert_eq!(
            snapshot
                .reconcile(&account("90.00", "40.00", "60.00"))
                .unwrap_err()
                .to_string(),
            "应收分录净额与账户应收总额不一致"
        );
        assert_eq!(
            snapshot
                .reconcile(&account("100.00", "40.00", "10.00"))
                .unwrap_err()
                .to_string(),
            "发票分配净额与账户已开票总额不一致"
        );
    }

    #[test]
    fn apply_and_reverse_nets_conserve_four_sides() {
        let snapshot = ReceivableFundsSnapshot::from_facts(
            vec![entry("e-1", 1, "100.00")],
            vec![
                receipt_alloc("a-1", "e-1", 1, "50.00", AllocationAction::Apply),
                receipt_alloc("a-2", "e-1", 2, "10.00", AllocationAction::Reverse),
            ],
            vec![
                invoice_alloc("i-1", 1, "30.00", AllocationAction::Apply),
                invoice_alloc("i-2", 2, "5.00", AllocationAction::Reverse),
            ],
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(snapshot.receipt_net().unwrap(), amount("40.00"));
        assert_eq!(snapshot.invoice_net().unwrap(), amount("25.00"));
        snapshot.reconcile(&account("100.00", "40.00", "25.00")).unwrap();
    }

    #[test]
    fn historical_plan_consumes_open_balance_in_sequence() {
        let snapshot = ReceivableFundsSnapshot::from_facts(
            vec![entry("e-2", 2, "30.00"), entry("e-1", 1, "50.00")],
            vec![receipt_alloc("a-1", "e-1", 1, "20.00", AllocationAction::Apply)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let plan = snapshot
            .plan_historical_receipt_allocations(amount("50.00"))
            .unwrap();
        assert_eq!(
            plan,
            vec![
                (ReceivableEntryId::new("e-1"), amount("30.00")),
                (ReceivableEntryId::new("e-2"), amount("20.00")),
            ]
        );
    }

    #[test]
    fn historical_plan_rejects_non_positive_over_allocated_and_insufficient() {
        let snapshot = ReceivableFundsSnapshot::from_facts(
            vec![entry("e-1", 1, "10.00")],
            vec![receipt_alloc("a-1", "e-1", 1, "10.00", AllocationAction::Apply)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(
            snapshot
                .plan_historical_receipt_allocations(amount("0.00"))
                .unwrap_err()
                .to_string(),
            "回款金额必须大于零"
        );
        assert_eq!(
            snapshot
                .plan_historical_receipt_allocations(amount("1.00"))
                .unwrap_err()
                .to_string(),
            "应收分录开放余额不足，无法完成历史回款分配"
        );

        let over = ReceivableFundsSnapshot::from_facts(
            vec![entry("e-1", 1, "10.00")],
            vec![receipt_alloc("a-1", "e-1", 1, "12.00", AllocationAction::Apply)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(
            over.plan_historical_receipt_allocations(amount("1.00"))
                .unwrap_err()
                .to_string(),
            "应收分录历史核销累计超过分录金额"
        );
    }

    #[test]
    fn duplicate_entry_and_invoice_are_rejected() {
        let duplicate_entry = entry("e-1", 2, "5.00");
        assert!(ReceivableFundsSnapshot::from_facts(
            vec![entry("e-1", 1, "10.00"), duplicate_entry],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .is_err());

        let invoice = Invoice::new(
            InvoiceId::new("inv-1"),
            InvoiceData {
                invoice_direction: InvoiceDirection::Sales,
                invoice_kind: InvoiceKind::Blue,
                party_id: PartyId::new("party-1"),
                invoice_code: None,
                invoice_no: "FP-1".to_string(),
                invoice_date: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                gross_amount: amount("10.00"),
                net_amount: amount("10.00"),
                tax_amount: amount("0.00"),
                rounding_adjustment_amount: amount("0.00"),
                rounding_reason: None,
                original_invoice_id: None,
            },
            "tester",
        )
        .unwrap();
        assert!(ReceivableFundsSnapshot::from_facts(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![invoice.clone(), invoice],
        )
        .is_err());
    }

    #[test]
    fn fact_version_is_deterministic_and_order_insensitive() {
        let left = ReceivableFundsSnapshot::from_facts(
            vec![entry("e-2", 2, "30.00"), entry("e-1", 1, "70.00")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let right = ReceivableFundsSnapshot::from_facts(
            vec![entry("e-1", 1, "70.00"), entry("e-2", 2, "30.00")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let acct = account("100.00", "0.00", "0.00");
        assert_eq!(left.fact_version(&acct), right.fact_version(&acct));
        assert!(left.fact_version(&acct).starts_with("ffv:"));
        let mut receipt = CustomerReceipt::new(
            CustomerReceiptId::new("cr-1"),
            CustomerReceiptData {
                receipt_no: "SK-1".to_string(),
                counterparty_party_id: PartyId::new("party-1"),
                customer_id: None,
                received_at: Instant::from_unix_secs(1_700_000_000),
                amount: amount("10.00"),
                bank_reference: None,
            },
            "tester",
        )
        .unwrap();
        receipt.register_historical_fact().unwrap();
        let with_receipt = ReceivableFundsSnapshot::from_facts(
            vec![entry("e-1", 1, "70.00"), entry("e-2", 2, "30.00")],
            Vec::new(),
            Vec::new(),
            vec![receipt],
            Vec::new(),
        )
        .unwrap();
        assert_ne!(left.fact_version(&acct), with_receipt.fact_version(&acct));
    }

    #[test]
    fn empty_snapshot_fact_version_is_stable() {
        let snapshot = empty_snapshot();
        let acct = account("0.00", "0.00", "0.00");
        assert_eq!(snapshot.fact_version(&acct), snapshot.fact_version(&acct));
    }
}
