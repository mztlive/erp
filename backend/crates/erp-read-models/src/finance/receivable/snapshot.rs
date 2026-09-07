//! Sales snapshot adapter for finance posting and receivable detail composition.

use crate::{Error, Result};
use erp_core::ids::ReceivableAccountId;
use erp_finance::entity::receivable::ReceivableAccount;
use erp_finance::repository::ReceivableExt;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

pub use erp_finance::ports::receivable::CardFundsSnapshot;
pub use erp_finance::service::receivable::mapping::{
    card_funds_review_chain, card_funds_snapshot_of, ensure_expected_version, invoice_fact_views,
    map_chain_error, map_ledger_error, parse_task_version, pending_review_status, receipt_fact_views,
    zero_amount,
};

/// 读取 W13 当前销售版本、账户分录、票款分配和复核链。
pub async fn load_card_funds_snapshot(
    db: &Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<CardFundsSnapshot> {
    let sales_order_id = account.sales_order_id.to_string();
    let sales_order = db
        .sales_orders()
        .find_by_id(&sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("应收账户来源销售单不存在".to_string()))?;
    let current_sales_order_revision_id = sales_order
        .stable
        .current_revision_id
        .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前正式版本".to_string()))?;
    let current_revision = db
        .sales_order_revisions()
        .find_by_id(&current_sales_order_revision_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单当前正式版本不存在".to_string()))?;
    let account_id = ReceivableAccountId::new(account.base.id.clone());
    let facts = db
        .receivable()
        .card_funds_snapshot_facts(&account_id, executor)
        .await?;
    if facts.receipts.len() != facts.expected_receipt_count {
        return Err(Error::NotFound("应收账户引用的回款单不存在".to_string()));
    }
    if facts.invoices.len() != facts.expected_invoice_count {
        return Err(Error::NotFound("应收账户引用的发票不存在".to_string()));
    }
    let entries = facts.entries;
    let receipt_allocations = facts.receipt_allocations;
    let invoice_allocations = facts.invoice_allocations;
    let receipts = facts.receipts;
    let invoices = facts.invoices;
    let reviews = facts.reviews;
    Ok(CardFundsSnapshot {
        current_sales_order_revision_id,
        sales_order_no: sales_order.order_no,
        sales_order_revision_no: current_revision.revision.revision_no,
        sales_order_snapshot_at: u64::try_from(current_revision.effective_at.unix_secs()).unwrap_or_default(),
        customer_name: current_revision.customer_snapshot.customer_name,
        counterparty_party_name: current_revision
            .settlement_party_snapshot
            .map(|snapshot| snapshot.settlement_party_name),
        entries,
        reviews,
        receipt_allocations,
        invoice_allocations,
        receipts,
        invoices,
    })
}
