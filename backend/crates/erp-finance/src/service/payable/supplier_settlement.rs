//! 供应商结算确认的财务应付构造与原账户/分录写入。

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{PayableAccountId, PayableEntryId, SupplierAccountId};
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
/// 财务创建结算应付实际消费的来源字段，不依赖供应链实体。
pub struct SettlementPayableSource {
    pub statement_no: String,
    pub supplier_id: SupplierAccountId,
    pub subject_hash: String,
    pub period_end: BusinessDate,
}
fn zero_amount() -> Amount {
    Amount::zero()
}
/// 按原 Account ID/new、Entry ID/new 时点构造结算应付；posted_at 复用调用者冻结时间。
pub fn build_settlement_payable(
    source: &SettlementPayableSource,
    amount: Amount,
    actor_id: &str,
    at: Instant,
) -> Result<(PayableAccount, PayableEntry)> {
    let account = PayableAccount::new(
        PayableAccountId::new(next_id()),
        PayableAccountData {
            source_document_id: source.statement_no.clone(),
            supplier_id: source.supplier_id.clone(),
            source_type: PayableSourceType::SupplierSettlement,
            gross_total: amount,
            settled_total: zero_amount(),
            invoiceable_total: amount,
            invoiced_total: zero_amount(),
        },
        actor_id,
    )?;
    let entry = PayableEntry::new(
        PayableEntryId::new(next_id()),
        PayableEntryData {
            payable_account_id: account.base.id.clone().into(),
            entry_type: PayableEntryType::Original,
            direction: EntryDirection::Increase,
            amount,
            due_date: source.period_end,
            source_fact_type: "supplier_settlement".to_string(),
            source_document_id: source.statement_no.clone(),
            source_revision_id: source.subject_hash.clone(),
            source_sequence: 1,
            posted_at: at,
        },
    )?;
    Ok((account, entry))
}

/// 使用调用者原事务执行器，复用财务仓储账户后分录的真实写序。
pub async fn persist_settlement_payable(
    db: &Database,
    account: &PayableAccount,
    entry: &PayableEntry,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.payable().create_payable_with_entry(account, entry, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    #[test]
    fn settlement_payable_preserves_source_money_due_date_and_posting_time() {
        let source = SettlementPayableSource {
            statement_no: "ST-2026-001".into(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            subject_hash: "a".repeat(64),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
        };
        let at = Instant::from_unix_secs(1700000100);
        let amount = Amount::from_str("100.00").unwrap();
        let (account, entry) = build_settlement_payable(&source, amount, "reviewer-1", at).unwrap();
        assert_eq!(account.source_type, PayableSourceType::SupplierSettlement);
        assert_eq!(account.source_document_id, source.statement_no);
        assert_eq!(account.supplier_id, source.supplier_id);
        assert_eq!(account.gross_total, amount);
        assert_eq!(account.invoiceable_total, amount);
        assert_eq!(account.settled_total, zero_amount());
        assert_eq!(account.invoiced_total, zero_amount());
        assert_eq!(entry.payable_account_id.as_ref(), account.base.id.as_str());
        assert_eq!(entry.entry_type, PayableEntryType::Original);
        assert_eq!(entry.direction, EntryDirection::Increase);
        assert_eq!(entry.amount, amount);
        assert_eq!(entry.due_date, source.period_end);
        assert_eq!(entry.source_fact_type, "supplier_settlement");
        assert_eq!(entry.source_document_id, source.statement_no);
        assert_eq!(entry.source_revision_id, source.subject_hash);
        assert_eq!(entry.source_sequence, 1);
        assert_eq!(entry.posted_at, at);
    }
}
