//! 采购变更差额的唯一应付准备与写入；仅消费冻结金额与稳定来源身份。

use std::str::FromStr;

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    PayableAccountId, PayableEntryId, PurchaseOrderId, PurchaseOrderRevisionId, SupplierAccountId,
};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::entity::payable::{
    EntryDirection, PayableAccount, PayableAccountData, PayableEntry, PayableEntryData, PayableEntryType,
    PayableSourceType,
};
use crate::repository::PayableExt;

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

/// 财务拥有本次追加的差额账户及分录；不改变现有账户。
#[derive(Debug)]
pub struct PurchaseChangePayableWrite {
    account: PayableAccount,
    entry: PayableEntry,
}
impl PurchaseChangePayableWrite {
    /// 返回原响应所需的差额分录稳定引用，不分配第二个身份。
    pub fn entry_id(&self) -> &str {
        &self.entry.base.id
    }

    /// 使用原多集合财务接口写入账户后写分录，复用调用方执行器。
    ///
    /// # 错误
    /// 保留原唯一键、事务与仓储错误，不起独立事务。
    pub async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        db.payable().create_payable_with_entry(&self.account, &self.entry, executor).await?;
        Ok(())
    }
}

/// 按原时点生成应付差额：零值无 ID/时钟调用，非零先账户校验再分录身份与日期。
///
/// # 错误
/// 账户不接受负金额的原校验保持；本迁移不为减额添加新的退款或贷项行为。
pub fn prepare_purchase_change_payable(
    input: PurchaseChangePayableInput,
) -> Result<Option<PurchaseChangePayableWrite>> {
    build(input, next_id, BusinessDate::today, Instant::now)
}

/// 注入的身份与时钟用于验证真实构造顺序；生产入口使用原生成器。
fn build(
    input: PurchaseChangePayableInput,
    mut next_identity: impl FnMut() -> String,
    today: impl FnOnce() -> BusinessDate,
    now: impl FnOnce() -> Instant,
) -> Result<Option<PurchaseChangePayableWrite>> {
    let delta_amount = Amount::try_from(input.new_gross.to_decimal() - input.base_gross.to_decimal())
        .expect("金额差值小数位不超过 2 位");
    if delta_amount.to_decimal() == zero_amount().to_decimal() {
        return Ok(None);
    }
    let account = PayableAccount::new(
        PayableAccountId::new(next_identity()),
        PayableAccountData {
            source_document_id: input.purchase_order_id.to_string(),
            supplier_id: input.supplier_id,
            source_type: PayableSourceType::PurchaseOrder,
            gross_total: delta_amount,
            settled_total: zero_amount(),
            invoiceable_total: delta_amount,
            invoiced_total: zero_amount(),
        },
        "system",
    )?;
    let entry = PayableEntry::new(
        PayableEntryId::new(next_identity()),
        PayableEntryData {
            payable_account_id: account.base.id.clone().into(),
            entry_type: PayableEntryType::ChangeDelta,
            direction: if delta_amount.to_decimal() > zero_amount().to_decimal() {
                EntryDirection::Increase
            } else {
                EntryDirection::Decrease
            },
            amount: Amount::try_from(delta_amount.to_decimal().abs()).expect("差额绝对值小数位不超过 2 位"),
            due_date: today(),
            source_fact_type: "purchase_change_order".to_string(),
            source_document_id: input.purchase_order_id.to_string(),
            source_revision_id: input.revision_id.to_string(),
            source_sequence: 1,
            posted_at: now(),
        },
    )?;
    Ok(Some(PurchaseChangePayableWrite { account, entry }))
}

/// 保留原财务构造使用的零金额精度。
fn zero_amount() -> Amount {
    Amount::from_str("0").expect("零金额合法")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    fn input(base: &str, new: &str) -> PurchaseChangePayableInput {
        PurchaseChangePayableInput {
            purchase_order_id: PurchaseOrderId::new("po-1"),
            supplier_id: SupplierAccountId::new("supplier-1"),
            revision_id: PurchaseOrderRevisionId::new("revision-2"),
            base_gross: Amount::from_str(base).unwrap(),
            new_gross: Amount::from_str(new).unwrap(),
        }
    }

    #[test]
    fn positive_delta_preserves_source_amount_and_identity_clock_order() {
        let calls = RefCell::new(Vec::new());
        let mut ids = ["account-1", "entry-1"].into_iter();
        let date = BusinessDate::from_str("2026-09-07").unwrap();
        let at = Instant::from_unix_secs(600);
        let write = build(
            input("100", "112.34"),
            || {
                let id = ids.next().unwrap();
                calls.borrow_mut().push(id);
                id.to_string()
            },
            || {
                calls.borrow_mut().push("date");
                date
            },
            || {
                calls.borrow_mut().push("now");
                at
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(*calls.borrow(), vec!["account-1", "entry-1", "date", "now"]);
        assert_eq!(write.account.gross_total, Amount::from_str("12.34").unwrap());
        assert_eq!(write.account.settled_total, zero_amount());
        assert_eq!(write.account.invoiced_total, zero_amount());
        assert_eq!(write.account.base.id, "account-1");
        assert_eq!(write.account.source_document_id, "po-1");
        assert_eq!(write.entry_id(), "entry-1");
        assert_eq!(write.entry.direction, EntryDirection::Increase);
        assert_eq!(write.entry.amount, Amount::from_str("12.34").unwrap());
        assert_eq!(write.entry.due_date, date);
        assert_eq!(write.entry.posted_at, at);
        assert_eq!(write.entry.source_fact_type, "purchase_change_order");
        assert_eq!(write.entry.source_document_id, "po-1");
        assert_eq!(write.entry.source_revision_id, "revision-2");
        assert_eq!(write.entry.source_sequence, 1);
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
    fn negative_delta_keeps_original_account_error_before_entry_identity_and_clock() {
        let calls = RefCell::new(Vec::new());
        let error = build(
            input("100", "99"),
            || {
                calls.borrow_mut().push("account");
                "account-1".into()
            },
            || panic!("账户校验失败不得读日期"),
            || panic!("账户校验失败不得取时"),
        )
        .unwrap_err();
        assert_eq!(*calls.borrow(), vec!["account"]);
        assert!(matches!(error, crate::Error::Logic(_)));
    }
}
