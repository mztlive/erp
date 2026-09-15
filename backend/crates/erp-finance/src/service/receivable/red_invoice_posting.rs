//! Red invoice and receivable/payable reversal writes in the caller's transaction.

use erp_core::ids::{
    InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId, ReceivableAccountId, SalesInvoiceAllocationId,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::red_invoice_plan::aggregate_reversal_deltas;
use crate::entity::payable::{PurchaseInvoiceAllocation, PurchaseInvoiceAllocationData};
use crate::entity::receivable::{
    AllocationAction, Invoice, InvoiceDirection, RedInvoiceAllocationPlan, SalesInvoiceAllocation,
    SalesInvoiceAllocationData,
};
use crate::repository::{PayableExt, ReceivableExt};
use crate::{Error, Result};

/// Write the registered red invoice and apply the validated reversal plan atomically.
///
/// Preserves invoice creation, optional original status update, account deltas and
/// allocation order. Returns affected receivable identities for workflow/sales steps.
/// The supplied executor is used for every operation; no transaction is opened here.
pub async fn persist_red_invoice(
    db: &Database,
    red_invoice: &Invoice,
    original_invoice: &mut Invoice,
    allocation_plan: &RedInvoiceAllocationPlan,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    db.invoices().create(red_invoice, executor).await?;
    if allocation_plan.is_full_reversal() {
        original_invoice.mark_red_invoiced(actor_id)?;
        db.invoices().update(original_invoice, executor).await?;
    }

    let mut sales_order_account_ids = Vec::new();
    // FIN-R11：按 direction 与 account 聚合 reversal delta 后批量
    // 条件更新与批量插入；同一 account 多行只更新一次。
    let reversal_deltas = aggregate_reversal_deltas(allocation_plan.lines());
    match original_invoice.invoice_direction {
        InvoiceDirection::Sales => {
            let deltas = reversal_deltas
                .iter()
                .map(|(account_id, gross)| (ReceivableAccountId::new(account_id.clone()), *gross))
                .collect::<Vec<_>>();
            let reverted =
                db.receivable_accounts().revert_invoicings_many(&deltas, actor_id, executor).await?;
            if !reverted.rejected.is_empty() {
                return Err(Error::BusinessLogicError("红冲金额超过已开票进度".to_string()));
            }
            let mut new_allocations = Vec::with_capacity(allocation_plan.lines().len());
            for (index, line) in allocation_plan.lines().iter().enumerate() {
                new_allocations.push(SalesInvoiceAllocation::new(
                    SalesInvoiceAllocationId::new(next_id()),
                    SalesInvoiceAllocationData {
                        invoice_id: InvoiceId::new(red_invoice.base.id.clone()),
                        receivable_account_id: ReceivableAccountId::new(line.account_id.clone()),
                        allocation_seq: (index as u32) + 1,
                        allocation_action: AllocationAction::Reverse,
                        allocated_gross_amount: line.gross,
                        allocated_net_amount: line.net,
                        allocated_tax_amount: line.tax,
                        reverses_allocation_id: Some(SalesInvoiceAllocationId::new(
                            line.original_allocation_id.clone(),
                        )),
                    },
                )?);
            }
            db.receivable().create_sales_invoice_allocations_many(&new_allocations, executor).await?;
            sales_order_account_ids.extend(reversal_deltas.iter().map(|(account_id, _)| account_id.clone()));
        },
        InvoiceDirection::Purchase => {
            let deltas = reversal_deltas
                .iter()
                .map(|(account_id, gross)| (PayableAccountId::new(account_id.clone()), *gross))
                .collect::<Vec<_>>();
            let reverted = db.payable_accounts().revert_invoicings_many(&deltas, actor_id, executor).await?;
            if !reverted.rejected.is_empty() {
                return Err(Error::BusinessLogicError("红冲金额超过已收票进度".to_string()));
            }
            let mut new_allocations = Vec::with_capacity(allocation_plan.lines().len());
            for (index, line) in allocation_plan.lines().iter().enumerate() {
                new_allocations.push(PurchaseInvoiceAllocation::new(
                    PurchaseInvoiceAllocationId::new(next_id()),
                    PurchaseInvoiceAllocationData {
                        invoice_id: InvoiceId::new(red_invoice.base.id.clone()),
                        payable_account_id: PayableAccountId::new(line.account_id.clone()),
                        allocation_seq: (index as u32) + 1,
                        allocation_action: crate::entity::payable::AllocationAction::Reverse,
                        allocated_gross_amount: line.gross,
                        allocated_net_amount: line.net,
                        allocated_tax_amount: line.tax,
                        reverses_allocation_id: Some(PurchaseInvoiceAllocationId::new(
                            line.original_allocation_id.clone(),
                        )),
                    },
                )?);
            }
            db.payable().create_purchase_invoice_allocations_many(&new_allocations, executor).await?;
        },
    }
    Ok(sales_order_account_ids)
}
