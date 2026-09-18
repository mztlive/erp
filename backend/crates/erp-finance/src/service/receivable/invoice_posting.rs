//! Sales invoice finance posting within the caller's existing transaction.

use std::collections::HashMap;

use erp_core::ids::SalesInvoiceAllocationId;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::receivable::sales_invoice_allocation_plan::SalesInvoiceAllocationLine;
use crate::entity::receivable::{Invoice, ReceivableAccount, SalesInvoiceAllocationPlan};
use crate::repository::ReceivableExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// Apply invoice allocation amounts, register the invoice and persist allocation facts.
///
/// The caller must pass its existing transaction executor. Returns affected accounts and
/// their original plan order for subsequent sales/workflow composition. Errors preserve
/// the prior missing-account, counterparty and insufficient-amount semantics.
pub async fn persist_sales_invoice_allocations(
    db: &Database,
    invoice: &mut Invoice,
    plan_lines: &[SalesInvoiceAllocationLine],
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<(Vec<ReceivableAccount>, Vec<String>)> {
    let allocation_ids: Vec<SalesInvoiceAllocationId> =
        (0..plan_lines.len()).map(|_| SalesInvoiceAllocationId::new(next_id())).collect();
    let plan = SalesInvoiceAllocationPlan::new(
        invoice.base.id.clone().into(),
        invoice.gross_amount,
        invoice.net_amount,
        invoice.tax_amount,
        plan_lines,
        &allocation_ids,
    )?;
    let account_id_strs: Vec<String> =
        plan.account_invoicing_deltas().iter().map(|(id, _)| id.to_string()).collect();
    let accounts = db.receivable_accounts().find_accounts_by_ids(&account_id_strs, executor).await?;
    let accounts_by_id: HashMap<&str, &ReceivableAccount> =
        accounts.iter().map(|account| (account.base.id.as_str(), account)).collect();
    for (account_id, _) in plan.account_invoicing_deltas() {
        let account = accounts_by_id
            .get(account_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        if account.counterparty_party_id != invoice.party_id {
            return Err(Error::BusinessLogicError("禁止跨往来主体开票".to_string()));
        }
    }
    let invoicing = db
        .receivable_accounts()
        .apply_invoicings_many(plan.account_invoicing_deltas(), actor_id, executor)
        .await?;
    if !invoicing.rejected.is_empty() {
        return Err(Error::BusinessLogicError("子账剩余可开票额度不足，开票被拒绝".to_string()));
    }
    invoice.mark_registered(actor_id)?;
    db.invoices().update(invoice, executor).await?;
    db.receivable().create_sales_invoice_allocations_many(plan.new_allocations(), executor).await?;
    Ok((accounts, account_id_strs))
}
