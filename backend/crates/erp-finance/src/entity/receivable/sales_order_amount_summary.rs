//! 销售单应收三金额摘要：按子账精确相加的存储无关投影。
//!
//! Repository 聚合结果必须与 [`SalesOrderReceivableAmountSummary::from_accounts`]
//! 逐项精确一致；无子账为精确零且 `account_count = 0`。

use erp_core::money::Amount;
use serde::{Deserialize, Serialize};

use super::receivable_account::ReceivableAccount;

/// 按销售单折叠的应收已核销/已开票/含税合计。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrderReceivableAmountSummary {
    /// 未删除子账数量；无子账为 `0`。
    pub account_count: u32,
    /// 已核销含税合计。
    pub settled_total: Amount,
    /// 净已开含税合计。
    pub invoiced_total: Amount,
    /// 含税应收合计。
    pub gross_total: Amount,
}

impl SalesOrderReceivableAmountSummary {
    /// 返回无子账时的精确零摘要。
    ///
    /// # 返回
    /// 返回 `account_count = 0` 且三金额均为精确零的摘要。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 无子账不得被结案规则解释为已结清；调用方必须读取 `has_accounts`。
    pub fn empty() -> Self {
        Self {
            account_count: 0,
            settled_total: Amount::zero(),
            invoiced_total: Amount::zero(),
            gross_total: Amount::zero(),
        }
    }

    /// 是否存在未删除应收子账。
    ///
    /// # 返回
    /// `account_count > 0` 时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn has_accounts(&self) -> bool {
        self.account_count > 0
    }

    /// 按实体字段精确相加构造摘要（聚合对拍基准算法）。
    ///
    /// # 参数
    /// * `accounts` - 同一销售单的未删除应收子账
    ///
    /// # 返回
    /// 返回子账数量与三金额精确合计；空迭代器返回 [`Self::empty`]。
    ///
    /// # 错误
    /// 无；金额值对象保持定点精度。
    ///
    /// # 关键业务约束
    /// 必须与 MongoDB Decimal128 `$sum` 结果逐项相等，禁止浮点或舍入。
    pub fn from_accounts<'a, I>(accounts: I) -> Self
    where
        I: IntoIterator<Item = &'a ReceivableAccount>,
    {
        let mut summary = Self::empty();
        for account in accounts {
            summary.account_count = summary.account_count.saturating_add(1);
            summary.settled_total = summary.settled_total.checked_add(account.settled_total);
            summary.invoiced_total = summary.invoiced_total.checked_add(account.invoiced_total);
            summary.gross_total = summary.gross_total.checked_add(account.gross_total);
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{
        CustomerAccountId, PartyId, ReceivableAccountId, SalesOrderId, SalesOrderRevisionId,
    };
    use erp_core::money::Amount;

    use super::SalesOrderReceivableAmountSummary;
    use crate::entity::receivable::SalesBusinessTypeFact as BusinessType;
    use crate::entity::receivable::receivable_account::{
        AccountReviewStatus, ReceivableAccount, ReceivableAccountData,
    };

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn account(id: &str, seq: u32, settled: &str, invoiced: &str, gross: &str) -> ReceivableAccount {
        ReceivableAccount::new(
            ReceivableAccountId::new(id),
            ReceivableAccountData {
                sales_order_id: SalesOrderId::new("so-1"),
                account_seq: seq,
                customer_id: CustomerAccountId::new("cust-1"),
                counterparty_party_id: PartyId::new("party-1"),
                source_sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                review_status: AccountReviewStatus::initial_for_sales_business_type(
                    BusinessType::GoodsService,
                ),
                reviewed_by: None,
                reviewed_at: None,
                review_evidence_reference: None,
                gross_total: amt(gross),
                settled_total: amt(settled),
                invoiceable_total: amt(gross),
                invoiced_total: amt(invoiced),
            },
            "tester",
        )
        .unwrap()
    }

    #[test]
    fn empty_accounts_are_precise_zero_and_not_present() {
        let none: [&ReceivableAccount; 0] = [];
        let summary = SalesOrderReceivableAmountSummary::from_accounts(none);
        assert_eq!(summary, SalesOrderReceivableAmountSummary::empty());
        assert!(!summary.has_accounts());
        assert_eq!(summary.settled_total, amt("0.00"));
        assert_eq!(summary.invoiced_total, amt("0.00"));
        assert_eq!(summary.gross_total, amt("0.00"));
    }

    #[test]
    fn single_and_multiple_accounts_match_checked_add() {
        let one = account("ra-1", 1, "10.01", "1.10", "20.05");
        let single = SalesOrderReceivableAmountSummary::from_accounts([&one]);
        assert!(single.has_accounts());
        assert_eq!(single.account_count, 1);
        assert_eq!(single.settled_total, one.settled_total);
        assert_eq!(single.invoiced_total, one.invoiced_total);
        assert_eq!(single.gross_total, one.gross_total);

        let two = account("ra-2", 2, "0.02", "0.00", "3.33");
        let many = SalesOrderReceivableAmountSummary::from_accounts([&one, &two]);
        assert_eq!(many.account_count, 2);
        assert_eq!(many.settled_total, one.settled_total.checked_add(two.settled_total));
        assert_eq!(many.invoiced_total, one.invoiced_total.checked_add(two.invoiced_total));
        assert_eq!(many.gross_total, one.gross_total.checked_add(two.gross_total));
        assert_eq!(many.settled_total, amt("10.03"));
        assert_eq!(many.gross_total, amt("23.38"));
    }

    #[test]
    fn zero_amount_account_still_counts_as_present() {
        let zero = account("ra-0", 1, "0.00", "0.00", "0.00");
        let summary = SalesOrderReceivableAmountSummary::from_accounts([&zero]);
        assert!(summary.has_accounts());
        assert_eq!(summary.account_count, 1);
        assert_eq!(summary.settled_total, amt("0.00"));
        assert_eq!(summary.gross_total, amt("0.00"));
    }
}
