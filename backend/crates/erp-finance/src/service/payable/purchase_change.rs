//! 采购变更差额的唯一应付准备与事务内追加；消费冻结金额与稳定来源身份。

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    PayableAccountId, PayableEntryId, PurchaseOrderId, PurchaseOrderRevisionId, SupplierAccountId,
};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::payable::{
    PayableAccount, PayableEntry, PayableEntryData, PayableEntryType, PurchaseChangePayableDelta,
};
use crate::repository::PayableExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 差额必须来自变更冻结的基准版本及目标版本；不接收采购聚合。
#[derive(Debug, Clone)]
pub struct PurchaseChangePayableInput {
    /// 原采购单，是账户和差额分录的来源单据。
    pub purchase_order_id: PurchaseOrderId,
    /// 原采购供应商账户。
    pub supplier_id: SupplierAccountId,
    /// 本次目标采购版本。
    pub revision_id: PurchaseOrderRevisionId,
    /// 冻结基准版本含税金额。
    pub base_gross: Amount,
    /// 目标版本含税金额。
    pub new_gross: Amount,
}

/// 财务持有冻结差额及稳定分录身份；既有子账在写入事务中读取。
#[derive(Debug)]
pub struct PurchaseChangePayableWrite {
    input: PurchaseChangePayableInput,
    delta: PurchaseChangePayableDelta,
    entry_id: PayableEntryId,
    due_date: BusinessDate,
    posted_at: Instant,
}
impl PurchaseChangePayableWrite {
    /// 返回本次差额分录的稳定身份。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已准备的分录身份，不分配第二个身份。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn entry_id(&self) -> &str {
        self.entry_id.as_ref()
    }

    /// 在调用方事务中读取原子账，追加差额分录并 CAS 更新原子账。
    ///
    /// # 参数
    /// * `db` - 财务事实所在数据库
    /// * `executor` - 采购变更生效事务的同一执行器
    ///
    /// # 返回
    /// 成功时返回已更新的原应付子账身份，供组合层同步付款任务。
    ///
    /// # 错误
    /// 缺少原子账、金额规则、唯一键、CAS 或仓储错误由外层事务回滚。
    pub async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<PayableAccountId> {
        let existing =
            db.payable_accounts().find_by_purchase_order(&self.input.purchase_order_id, executor).await?;
        let (mut account, entry) = self.build_posting(existing)?;
        db.payable_entries().create(&entry, executor).await?;
        db.payable_accounts().update(&mut account, executor).await?;
        Ok(account.base.id.into())
    }

    /// 将事务内读取的原子账及冻结差额转为实际写入内容。
    fn build_posting(&self, existing: Option<PayableAccount>) -> Result<(PayableAccount, PayableEntry)> {
        let mut account = existing
            .ok_or_else(|| Error::BusinessLogicError("采购单缺少正式应付子账，不能生效采购变更".into()))?;
        account
            .apply_purchase_change(
                &self.input.purchase_order_id,
                &self.input.supplier_id,
                self.input.base_gross,
                self.input.new_gross,
            )
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let entry = PayableEntry::new(
            self.entry_id.clone(),
            PayableEntryData {
                payable_account_id: account.base.id.clone().into(),
                entry_type: PayableEntryType::ChangeDelta,
                direction: self.delta.direction,
                amount: self.delta.amount,
                due_date: self.due_date,
                source_fact_type: "purchase_change_order".to_string(),
                source_document_id: self.input.purchase_order_id.to_string(),
                source_revision_id: self.input.revision_id.to_string(),
                source_sequence: 1,
                posted_at: self.posted_at,
            },
        )?;
        Ok((account, entry))
    }
}

/// 准备冻结差额及分录身份；零差额不分配身份、不读取时钟。
///
/// # 参数
/// * `input` - 基准与目标采购版本的冻结金额及来源身份
///
/// # 返回
/// 零差额返回 `None`，非零返回在调用方事务内追加到原子账的写入计划。
///
/// # 错误
/// 版本金额非法时返回错误；原子账及核销约束在事务内读取后校验。
pub fn prepare_purchase_change_payable(
    input: PurchaseChangePayableInput,
) -> Result<Option<PurchaseChangePayableWrite>> {
    build(input, || PayableEntryId::new(next_id()), BusinessDate::today, Instant::now)
}

/// 生产使用原身份和时钟来源，单测注入值验证零差额与确定身份。
fn build(
    input: PurchaseChangePayableInput,
    next_identity: impl FnOnce() -> PayableEntryId,
    today: impl FnOnce() -> BusinessDate,
    now: impl FnOnce() -> Instant,
) -> Result<Option<PurchaseChangePayableWrite>> {
    let Some(delta) = PurchaseChangePayableDelta::between(input.base_gross, input.new_gross)? else {
        return Ok(None);
    };
    Ok(Some(PurchaseChangePayableWrite {
        input,
        delta,
        entry_id: next_identity(),
        due_date: today(),
        posted_at: now(),
    }))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::str::FromStr;

    use super::*;
    use crate::entity::payable::{
        EntryDirection, PayableAccountData, PayableAccountStatus, PayableSourceType,
    };

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }
    fn input(base: &str, new: &str) -> PurchaseChangePayableInput {
        PurchaseChangePayableInput {
            purchase_order_id: PurchaseOrderId::new("po-1"),
            supplier_id: SupplierAccountId::new("supplier-1"),
            revision_id: PurchaseOrderRevisionId::new("revision-2"),
            base_gross: amount(base),
            new_gross: amount(new),
        }
    }
    fn account(settled: &str, invoiced: &str) -> PayableAccount {
        let mut account = PayableAccount::new(
            PayableAccountId::new("account-1"),
            PayableAccountData {
                source_document_id: "po-1".into(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: amount("100"),
                settled_total: amount(settled),
                invoiceable_total: amount("100"),
                invoiced_total: amount(invoiced),
            },
            "creator-1",
        )
        .unwrap();
        account.base.version = 9;
        account.stable.current_revision_id = Some("payable-revision-1".into());
        account
    }
    fn prepared(base: &str, new: &str) -> PurchaseChangePayableWrite {
        build(
            input(base, new),
            || PayableEntryId::new("entry-1"),
            || BusinessDate::from_str("2026-09-07").unwrap(),
            || Instant::from_unix_secs(600),
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn positive_and_negative_deltas_update_original_account_and_preserve_finance_facts() {
        for (new, direction, absolute) in
            [("112.34", EntryDirection::Increase, "12.34"), ("87.66", EntryDirection::Decrease, "12.34")]
        {
            let original = account("40", "30");
            let write = prepared("100", new);
            let (account, entry) = write.build_posting(Some(original.clone())).unwrap();
            assert_eq!(account.base, original.base);
            assert_eq!(account.stable.created_by, "creator-1");
            assert_eq!(account.stable.updated_by, "system");
            assert_eq!(account.stable.current_revision_id, original.stable.current_revision_id);
            assert_eq!(account.source_document_id, "po-1");
            assert_eq!(account.supplier_id, original.supplier_id);
            assert_eq!(account.source_type, PayableSourceType::PurchaseOrder);
            assert_eq!(account.gross_total, amount(new));
            assert_eq!(account.invoiceable_total, amount(new));
            assert_eq!(account.settled_total, amount("40"));
            assert_eq!(account.invoiced_total, amount("30"));
            assert_eq!(account.open_total.checked_add(account.settled_total), account.gross_total);
            assert_eq!(
                account.open_invoiceable_total.checked_add(account.invoiced_total),
                account.invoiceable_total
            );
            assert_eq!(account.stable.status, PayableAccountStatus::PartiallySettled);
            assert_eq!(write.entry_id(), "entry-1");
            assert_eq!(entry.payable_account_id.as_ref(), "account-1");
            assert_eq!(entry.direction, direction);
            assert_eq!(entry.amount, amount(absolute));
            assert_eq!(entry.entry_type, PayableEntryType::ChangeDelta);
            assert_eq!(entry.source_fact_type, "purchase_change_order");
            assert_eq!(entry.source_document_id, "po-1");
            assert_eq!(entry.source_revision_id, "revision-2");
            assert_eq!(entry.source_sequence, 1);
            assert_eq!(entry.due_date, BusinessDate::from_str("2026-09-07").unwrap());
            assert_eq!(entry.posted_at, Instant::from_unix_secs(600));
            let signed_delta = match entry.direction {
                EntryDirection::Increase => entry.amount,
                EntryDirection::Decrease => amount("0").checked_sub(entry.amount),
            };
            assert_eq!(original.gross_total.checked_add(signed_delta), account.gross_total);
        }
    }

    #[test]
    fn zero_delta_never_allocates_identity_or_reads_clock() {
        let result = build(
            input("100", "100"),
            || panic!("零差额不得分配 ID"),
            || panic!("零差额不得读日期"),
            || panic!("零差额不得取时"),
        )
        .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn nonzero_delta_allocates_only_entry_identity_before_clock() {
        let calls = RefCell::new(Vec::new());
        let write = build(
            input("100", "99"),
            || {
                calls.borrow_mut().push("entry");
                PayableEntryId::new("entry-1")
            },
            || {
                calls.borrow_mut().push("date");
                BusinessDate::from_str("2026-09-07").unwrap()
            },
            || {
                calls.borrow_mut().push("now");
                Instant::from_unix_secs(600)
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(*calls.borrow(), vec!["entry", "date", "now"]);
        assert_eq!(write.delta.direction, EntryDirection::Decrease);
        assert_eq!(write.delta.amount, amount("1"));
    }

    #[test]
    fn missing_original_account_fails_closed_without_fabricating_delta_account() {
        for target in ["101", "99"] {
            let error = prepared("100", target).build_posting(None).unwrap_err();
            assert!(matches!(error, Error::BusinessLogicError(message)
                if message == "采购单缺少正式应付子账，不能生效采购变更"));
        }
    }

    #[test]
    fn reducing_below_settled_or_invoiced_facts_is_rejected_with_original_guard() {
        for (settled, invoiced, target, message) in [
            ("60", "0", "59.99", "已核销总额不得超过含税应付总额"),
            ("0", "60", "59.99", "净已收票金额不得超过可收票总额"),
            ("100", "0", "0", "已核销总额不得超过含税应付总额"),
        ] {
            let error = prepared("100", target).build_posting(Some(account(settled, invoiced))).unwrap_err();
            assert!(matches!(error, Error::BusinessLogicError(ref actual) if actual == message));
        }
        let (updated, _) = prepared("100", "60").build_posting(Some(account("60", "60"))).unwrap();
        assert_eq!(updated.open_total, amount("0"));
        assert_eq!(updated.open_invoiceable_total, amount("0"));
        assert_eq!(updated.stable.status, PayableAccountStatus::Settled);
    }

    #[test]
    fn wrong_source_supplier_or_frozen_base_rejects_original_account() {
        let original = account("0", "0");
        let candidates = [
            PayableAccount { source_document_id: "other-po".into(), ..original.clone() },
            PayableAccount { supplier_id: SupplierAccountId::new("other-supplier"), ..original.clone() },
            PayableAccount { source_type: PayableSourceType::SupplierSettlement, ..original.clone() },
        ];
        for candidate in candidates {
            let error = prepared("100", "101").build_posting(Some(candidate)).unwrap_err();
            assert!(matches!(error, Error::BusinessLogicError(ref message)
                if message == "采购变更应付来源或供应商不一致"));
        }
        let error = prepared("99", "101").build_posting(Some(original)).unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(ref message)
            if message == "应付总额与采购变更基准版本不一致"));
    }

    #[test]
    fn zero_target_keeps_original_identity_and_appends_full_decrease() {
        let (account, entry) = prepared("100", "0").build_posting(Some(account("0", "0"))).unwrap();
        assert_eq!(account.base.id, "account-1");
        assert_eq!(account.gross_total, amount("0"));
        assert_eq!(account.invoiceable_total, amount("0"));
        assert_eq!(account.open_total, amount("0"));
        assert_eq!(account.open_invoiceable_total, amount("0"));
        assert_eq!(account.stable.status, PayableAccountStatus::Settled);
        assert_eq!(entry.direction, EntryDirection::Decrease);
        assert_eq!(entry.amount, amount("100"));
    }

    #[test]
    fn same_plan_keeps_entry_identity_and_revision_payload_deterministic() {
        let write = prepared("100", "112.34");
        let original = account("0", "0");
        let first = write.build_posting(Some(original.clone())).unwrap();
        let second = write.build_posting(Some(original)).unwrap();
        assert_eq!(first.0, second.0);
        assert_eq!(first.1.base.id, second.1.base.id);
        assert_eq!(first.1.payable_account_id, second.1.payable_account_id);
        assert_eq!(first.1.source_fact_type, second.1.source_fact_type);
        assert_eq!(first.1.source_document_id, second.1.source_document_id);
        assert_eq!(first.1.source_revision_id, second.1.source_revision_id);
        assert_eq!(first.1.source_sequence, second.1.source_sequence);
        assert_eq!(first.1.entry_type, second.1.entry_type);
        assert_eq!(first.1.direction, second.1.direction);
        assert_eq!(first.1.amount, second.1.amount);
        assert_eq!(first.1.due_date, second.1.due_date);
        assert_eq!(first.1.posted_at, second.1.posted_at);
        let replay_on_changed = write.build_posting(Some(first.0)).unwrap_err();
        assert!(matches!(replay_on_changed, Error::BusinessLogicError(ref message)
            if message == "应付总额与采购变更基准版本不一致"));
    }

    #[test]
    fn negative_frozen_amounts_fail_before_identity_and_clock() {
        for (base, target) in [("-1", "1"), ("100", "-1")] {
            let error = build(
                input(base, target),
                || panic!("非法版本金额不得分配 ID"),
                || panic!("非法版本金额不得读日期"),
                || panic!("非法版本金额不得取时"),
            )
            .unwrap_err();
            assert!(matches!(error, Error::Logic(_)));
        }
    }
}
