//! 销售变更应收差额的唯一财务准备与写入接口；只消费冻结金额和稳定来源标识。

use crate::entity::receivable::{
    ReceivableAccount, ReceivableDelta, ReceivableEntry, ReceivableEntryData, ReceivableEntryType,
    SalesBusinessTypeFact,
};
use crate::repository::ReceivableExt;
use crate::{Error, Result};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{ReceivableEntryId, SalesOrderId, SalesOrderRevisionId};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

/// 财务消费方所需的销售修订事实，不接收销售单或正式版本聚合。
#[derive(Debug, Clone)]
pub struct SalesChangeReceivableInput {
    /// 原销售单标识，也是差额分录的来源单据标识。
    pub sales_order_id: SalesOrderId,
    /// 新正式版本标识。
    pub revision_id: SalesOrderRevisionId,
    /// 财务所需的业务分类，保留销售业务归属。
    pub business_type: SalesBusinessTypeFact,
    /// 当前生效版本含税金额，作为差额基准。
    pub current_gross: Amount,
    /// 新正式版本公共行含税总额。
    pub new_gross: Amount,
    /// 构造正式版本时冻结的入账时间。
    pub posted_at: Instant,
    /// 已授权的本次生效操作人。
    pub updated_by: String,
}

/// 已准备的单次差额写入；账户和分录由财务持有，流程只读取任务所需事实。
#[derive(Debug)]
pub struct SalesChangeReceivableWrite {
    account: ReceivableAccount,
    entry: ReceivableEntry,
}
impl SalesChangeReceivableWrite {
    /// 写入后的财务子账，供组合层继续处理开票任务。
    pub fn account(&self) -> &ReceivableAccount {
        &self.account
    }

    /// 本次差额的正式版本号，用于追溯本次金额调整。
    pub fn subject_version(&self) -> &str {
        &self.entry.source_revision_id
    }

    /// 按原顺序先创建差额分录，再 CAS 更新账户；复用调用方 Executor。
    ///
    /// # 错误
    /// 唯一键、CAS 与仓储错误保留原分类，由外层根事务回滚。
    pub async fn persist(&mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        db.receivable_entries().create(&self.entry, executor).await?;
        db.receivable_accounts()
            .update(&mut self.account, executor)
            .await?;
        Ok(())
    }
}

/// 读取既有主应收子账，再计算差额，不启动事务。
///
/// 子账必须先读取，即使新旧金额相等也保留原读取时点。零差额不产生分录；
/// 非零差额而子账缺失表示正式事实链损坏，必须拒绝，不能补造兼容账户。
/// 调用方显式提供原 NoTransaction 或已有 Executor，不暗中改变隔离边界。
///
/// # 错误
/// 保留原缺子账 BusinessLogicError、金额 Logic 及仓储错误。
pub async fn prepare_sales_change_receivable(
    db: &Database,
    input: SalesChangeReceivableInput,
    executor: &mut dyn Executor,
) -> Result<Option<SalesChangeReceivableWrite>> {
    let existing_account = db
        .receivable_accounts()
        .find_primary_by_sales_order(&input.sales_order_id, executor)
        .await?;
    build_sales_change_receivable(
        input,
        existing_account,
        || ReceivableEntryId::new(next_id()),
        BusinessDate::today,
    )
}

/// 构建应收差额分录（§8.1.3：新版本金额减当前版本金额，零差额不写）。
///
/// 差额方向、绝对金额与账户金额更新由 `ReceivableDelta` / `ReceivableAccount` 决定。
/// 先完成账户校验和更新，再分配分录 ID、读取到期业务日，保持原失败顺序。
fn build_sales_change_receivable(
    input: SalesChangeReceivableInput,
    existing_account: Option<ReceivableAccount>,
    entry_id: impl FnOnce() -> ReceivableEntryId,
    due_date: impl FnOnce() -> BusinessDate,
) -> Result<Option<SalesChangeReceivableWrite>> {
    let delta =
        ReceivableDelta::try_from_gross(input.new_gross, input.current_gross).map_err(Error::Logic)?;
    let Some(delta) = delta else {
        return Ok(None);
    };
    let mut account = existing_account
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少正式应收子账，不能生效销售变更".to_string()))?;
    let account_update = account
        .sales_change_delta_update(input.business_type, input.new_gross)
        .map_err(Error::Logic)?;
    account
        .update(account_update, &input.updated_by)
        .map_err(Error::Logic)?;
    let entry = ReceivableEntry::new(
        entry_id(),
        ReceivableEntryData {
            receivable_account_id: account.base.id.clone().into(),
            entry_type: ReceivableEntryType::SalesChangeDelta,
            direction: delta.direction(),
            amount: delta.absolute_amount(),
            due_date: due_date(),
            source_fact_type: "SALES_CHANGE".to_string(),
            source_document_id: input.sales_order_id.to_string(),
            source_revision_id: input.revision_id.to_string(),
            source_sequence: 1,
            posted_at: input.posted_at,
        },
    )
    .map_err(Error::Logic)?;
    Ok(Some(SalesChangeReceivableWrite { account, entry }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::receivable::{AccountReviewStatus, EntryDirection, ReceivableAccountData};
    use erp_core::ids::{CustomerAccountId, PartyId, ReceivableAccountId};
    use std::cell::Cell;
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }
    fn input(business_type: SalesBusinessTypeFact, gross: &str) -> SalesChangeReceivableInput {
        SalesChangeReceivableInput {
            sales_order_id: SalesOrderId::new("sales-1"),
            revision_id: SalesOrderRevisionId::new("revision-2"),
            business_type,
            current_gross: amount("100.00"),
            new_gross: amount(gross),
            posted_at: Instant::from_unix_secs(1_800_000_000),
            updated_by: "reviewer-1".to_string(),
        }
    }
    fn account(review_status: AccountReviewStatus, invoiced: &str) -> ReceivableAccount {
        let reviewed = review_status == AccountReviewStatus::Reviewed;
        ReceivableAccount::new(
            ReceivableAccountId::new("account-1"),
            ReceivableAccountData {
                sales_order_id: SalesOrderId::new("sales-1"),
                account_seq: 1,
                customer_id: CustomerAccountId::new("customer-1"),
                counterparty_party_id: PartyId::new("party-1"),
                source_sales_order_revision_id: SalesOrderRevisionId::new("revision-1"),
                review_status,
                reviewed_by: reviewed.then(|| "initial-reviewer".to_string()),
                reviewed_at: reviewed.then(|| Instant::from_unix_secs(10)),
                review_evidence_reference: reviewed.then(|| "initial-proof".to_string()),
                gross_total: amount("100.00"),
                settled_total: amount("0.00"),
                invoiceable_total: amount("100.00"),
                invoiced_total: amount(invoiced),
            },
            "creator-1",
        )
        .unwrap()
    }
    fn build(
        input: SalesChangeReceivableInput,
        account: Option<ReceivableAccount>,
    ) -> Result<Option<SalesChangeReceivableWrite>> {
        build_sales_change_receivable(
            input,
            account,
            || ReceivableEntryId::new("delta-1"),
            || BusinessDate::from_ymd(2026, 9, 7).unwrap(),
        )
    }

    #[test]
    fn positive_and_negative_changes_keep_exact_amounts_and_original_source_identity() {
        for (gross, direction, absolute) in [
            ("123.45", EntryDirection::Increase, "23.45"),
            ("76.55", EntryDirection::Decrease, "23.45"),
        ] {
            let write = build(
                input(SalesBusinessTypeFact::GoodsService, gross),
                Some(account(AccountReviewStatus::NotApplicable, "0.00")),
            )
            .unwrap()
            .unwrap();
            assert_eq!(write.entry.direction, direction);
            assert_eq!(write.entry.amount, amount(absolute));
            assert_eq!(write.entry.entry_type, ReceivableEntryType::SalesChangeDelta);
            assert_eq!(write.entry.source_fact_type, "SALES_CHANGE");
            assert_eq!(write.entry.source_document_id, "sales-1");
            assert_eq!(write.entry.source_revision_id, "revision-2");
            assert_eq!(write.entry.source_sequence, 1);
            assert_eq!(write.entry.posted_at, Instant::from_unix_secs(1_800_000_000));
            assert_eq!(write.account.gross_total, amount(gross));
            assert_eq!(write.account.invoiceable_total, amount(gross));
            assert_eq!(write.account.review_status, AccountReviewStatus::NotApplicable);
        }
    }

    #[test]
    fn zero_and_missing_account_keep_original_precedence_without_allocating_entry_identity() {
        let identity_reads = Cell::new(0);
        let date_reads = Cell::new(0);
        for gross in ["100.00", "100.01"] {
            let result = build_sales_change_receivable(
                input(SalesBusinessTypeFact::GoodsService, gross),
                None,
                || {
                    identity_reads.set(identity_reads.get() + 1);
                    ReceivableEntryId::new("unused")
                },
                || {
                    date_reads.set(date_reads.get() + 1);
                    BusinessDate::from_ymd(2026, 9, 7).unwrap()
                },
            );
            if gross == "100.00" {
                assert!(result.unwrap().is_none());
            } else {
                assert!(
                    matches!(result, Err(Error::BusinessLogicError(ref message)) if message == "销售单缺少正式应收子账，不能生效销售变更")
                );
            }
        }
        assert_eq!(identity_reads.get(), 0);
        assert_eq!(date_reads.get(), 0);
    }

    /// 卡券金额变更不受历史复核状态阻挡，且保留应收、开票金额约束。
    #[test]
    fn voucher_changes_ignore_retired_review_status_and_keep_money_guards() {
        for status in [
            AccountReviewStatus::NotApplicable,
            AccountReviewStatus::OpeningPending,
            AccountReviewStatus::Reviewed,
            AccountReviewStatus::SyncDeltaPending,
        ] {
            for (gross, direction) in [
                ("120.00", EntryDirection::Increase),
                ("80.00", EntryDirection::Decrease),
            ] {
                let write = build(
                    input(SalesBusinessTypeFact::Voucher, gross),
                    Some(account(status, "50.00")),
                )
                .unwrap()
                .unwrap();
                assert_eq!(write.account.review_status, AccountReviewStatus::NotApplicable);
                assert_eq!(write.account.gross_total, amount(gross));
                assert_eq!(write.account.invoiceable_total, amount(gross));
                assert_eq!(write.account.invoiced_total, amount("50.00"));
                assert_eq!(write.entry.direction, direction);
                assert_eq!(write.entry.amount, amount("20.00"));
            }
            assert!(build(
                input(SalesBusinessTypeFact::Voucher, "49.99"),
                Some(account(status, "50.00"))
            )
            .is_err());
        }
    }
}
