//! 销售首次生效的应收子账与原始分录；只消费冻结财务事实并复用调用方事务。

use std::str::FromStr;

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    CustomerAccountId, PartyId, ReceivableAccountId, ReceivableEntryId, SalesOrderId, SalesOrderRevisionId,
};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::receivable::{
    AccountReviewStatus, EntryDirection, ReceivableAccount, ReceivableAccountData, ReceivableEntry,
    ReceivableEntryData, ReceivableEntryType, SalesBusinessTypeFact,
};
use crate::repository::ReceivableExt;
use crate::{Error, Result};

/// 首次应收形成所需的冻结事实；不得携带销售聚合或审批、工作项对象。
#[derive(Debug, Clone)]
pub struct InitialReceivableInput {
    /// 销售业务分类，用于标识应收来源业务。
    pub business_type: SalesBusinessTypeFact,
    /// 来源销售单稳定标识。
    pub sales_order_id: SalesOrderId,
    /// 客户经营归属。
    pub customer_id: CustomerAccountId,
    /// 当前生效版本的收款、开票往来主体。
    pub counterparty_party_id: PartyId,
    /// 首次生效销售版本标识。
    pub source_sales_order_revision_id: SalesOrderRevisionId,
    /// 已冻结销售版本的含税总额。
    pub gross_total: Amount,
    /// 销售形式化时冻结的入账时间。
    pub posted_at: Instant,
}

/// 构建并写入销售单生效的应收往来子账与原始应收分录（§6.8/§8.1.1）。
///
/// 只应由首次生效（最终审批通过）路径调用：子账 `account_seq = 1`，分录类型
/// 为原始应收（增加方向，金额 = 版本含税合计）；后续销售变更差额由独立接口写入。
/// 本接口复用调用方 Executor，不开启事务；重复生效由外层提交状态守卫拦截。
///
/// # 返回
/// 返回已与原始分录一并持久化的子账，供组合层按原顺序推进财务任务。
///
/// # 错误
/// 实体不变量失败保持 `Logic`，仓储失败保留原唯一键、并发与基础设施错误类别。
pub async fn create_initial_receivable(
    db: &Database,
    input: InitialReceivableInput,
    executor: &mut dyn Executor,
) -> Result<ReceivableAccount> {
    let account_id = ReceivableAccountId::new(next_id());
    let entry_id = ReceivableEntryId::new(next_id());
    let (account, entry) = build_initial_receivable(input, account_id, entry_id, BusinessDate::today)?;
    db.receivable().create_receivable_with_entry(&account, &entry, executor).await?;
    Ok(account)
}

/// 先构造账户，再读取到期业务日并构造分录；保持原校验和取时顺序。
/// 日期提供器使纯内联测试固定业务日，生产仍在原位置调用 `BusinessDate::today`。
fn build_initial_receivable(
    input: InitialReceivableInput,
    account_id: ReceivableAccountId,
    entry_id: ReceivableEntryId,
    due_date: impl FnOnce() -> BusinessDate,
) -> Result<(ReceivableAccount, ReceivableEntry)> {
    let account = ReceivableAccount::new(
        account_id.clone(),
        ReceivableAccountData {
            sales_order_id: input.sales_order_id.clone(),
            account_seq: 1,
            customer_id: input.customer_id,
            counterparty_party_id: input.counterparty_party_id,
            source_sales_order_revision_id: input.source_sales_order_revision_id.clone(),
            review_status: AccountReviewStatus::initial_for_sales_business_type(input.business_type),
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: input.gross_total,
            settled_total: Amount::from_str("0.00").expect("静态零值必须合法"),
            invoiceable_total: input.gross_total,
            invoiced_total: Amount::from_str("0.00").expect("静态零值必须合法"),
        },
        "system",
    )
    .map_err(Error::Logic)?;
    let entry = ReceivableEntry::new(
        entry_id,
        ReceivableEntryData {
            receivable_account_id: account_id,
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: input.gross_total,
            due_date: due_date(),
            source_fact_type: "sales_order".to_string(),
            source_document_id: input.sales_order_id.to_string(),
            source_revision_id: input.source_sales_order_revision_id.to_string(),
            source_sequence: 1,
            posted_at: input.posted_at,
        },
    )
    .map_err(Error::Logic)?;
    Ok((account, entry))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::entity::receivable::ReceivableAccountStatus;

    fn input(business_type: SalesBusinessTypeFact, gross: &str) -> InitialReceivableInput {
        InitialReceivableInput {
            business_type,
            sales_order_id: SalesOrderId::new("sales-1"),
            customer_id: CustomerAccountId::new("customer-1"),
            counterparty_party_id: PartyId::new("party-1"),
            source_sales_order_revision_id: SalesOrderRevisionId::new("revision-1"),
            gross_total: Amount::from_str(gross).unwrap(),
            posted_at: Instant::from_unix_secs(1_720_000_000),
        }
    }

    fn business_date() -> BusinessDate {
        BusinessDate::from_ymd(2026, 9, 7).unwrap()
    }

    #[test]
    fn goods_and_voucher_keep_initial_review_state_and_original_source_identity() {
        for (business_type, review_status) in [
            (SalesBusinessTypeFact::GoodsService, AccountReviewStatus::NotApplicable),
            (SalesBusinessTypeFact::Voucher, AccountReviewStatus::NotApplicable),
        ] {
            let input = input(business_type, "123.45");
            let posted_at = input.posted_at;
            let (account, entry) = build_initial_receivable(
                input,
                ReceivableAccountId::new("account-1"),
                ReceivableEntryId::new("entry-1"),
                business_date,
            )
            .unwrap();
            assert_eq!(account.base.id, "account-1");
            assert_eq!(account.sales_order_id.as_ref(), "sales-1");
            assert_eq!(account.customer_id.as_ref(), "customer-1");
            assert_eq!(account.counterparty_party_id.as_ref(), "party-1");
            assert_eq!(account.source_sales_order_revision_id.as_ref(), "revision-1");
            assert_eq!(account.account_seq, 1);
            assert_eq!(account.review_status, review_status);
            assert_eq!(account.stable.created_by, "system");
            assert_eq!(account.stable.status(), ReceivableAccountStatus::Open);
            assert!(account.reviewed_by.is_none());
            assert!(account.reviewed_at.is_none());
            assert!(account.review_evidence_reference.is_none());
            assert_eq!(entry.base.id, "entry-1");
            assert_eq!(entry.receivable_account_id.as_ref(), account.base.id);
            assert_eq!(entry.entry_type, ReceivableEntryType::Original);
            assert_eq!(entry.direction, EntryDirection::Increase);
            assert_eq!(entry.source_fact_type, "sales_order");
            assert_eq!(entry.source_document_id, "sales-1");
            assert_eq!(entry.source_revision_id, "revision-1");
            assert_eq!(entry.source_sequence, 1);
            assert_eq!(entry.posted_at, posted_at);
            assert_eq!(entry.due_date, business_date());
        }
    }

    #[test]
    fn positive_amounts_remain_exact_and_initial_balances_remain_zero() {
        for gross in ["0.01", "123.45", "12345678.90"] {
            let input = input(SalesBusinessTypeFact::GoodsService, gross);
            let expected = input.gross_total;
            let (account, entry) = build_initial_receivable(
                input,
                ReceivableAccountId::new("account-1"),
                ReceivableEntryId::new("entry-1"),
                business_date,
            )
            .unwrap();
            assert_eq!(account.gross_total, expected);
            assert_eq!(account.invoiceable_total, expected);
            assert_eq!(account.open_total, expected);
            assert_eq!(account.open_invoiceable_total, expected);
            assert_eq!(entry.amount, expected);
            assert_eq!(account.settled_total, Amount::from_str("0.00").unwrap());
            assert_eq!(account.invoiced_total, Amount::from_str("0.00").unwrap());
        }
    }

    #[test]
    fn nonpositive_amounts_keep_domain_errors_and_validation_order() {
        for (gross, expected_message, expected_date_reads) in
            [("-0.01", "子账汇总金额不得为负", 0), ("0.00", "应收分录金额必须为正数", 1)]
        {
            let date_reads = Cell::new(0);
            let error = build_initial_receivable(
                input(SalesBusinessTypeFact::Voucher, gross),
                ReceivableAccountId::new("account-1"),
                ReceivableEntryId::new("entry-1"),
                || {
                    date_reads.set(date_reads.get() + 1);
                    business_date()
                },
            )
            .unwrap_err();
            assert!(matches!(error, Error::Logic(ref source) if source.to_string() == expected_message));
            assert_eq!(date_reads.get(), expected_date_reads);
        }
    }
}
