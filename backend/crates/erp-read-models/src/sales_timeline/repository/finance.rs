//! 沿实际分录和分配追溯资金；每个金额只来自本单关联的分配行。
use erp_finance::repository::{
    PayableExt, PurchasePaymentRequestExt, PurchasePaymentRequestSnapshotExt, ReceivableExt,
};
use erp_returns::repository::ReturnsExt;
use erp_workflow::DocumentType;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use super::super::TimelineCategory;
use super::super::finance::allocation;
use super::super::sources::Sources;
use crate::Result;

pub(in crate::sales_timeline) async fn discover(
    db: &Database,
    sale: &str,
    sources: &mut Sources,
    executor: &mut dyn Executor,
) -> Result<()> {
    receivable(db, sale, sources, executor).await?;
    payable(db, sale, sources, executor).await?;
    let receipts = sources.ids(DocumentType::CustomerReceipt);
    let payments = sources.ids(DocumentType::SupplierPayment);
    for row in db
        .receipt_reversals()
        .find_many(doc! { "original_customer_receipt_id": { "$in": &receipts } }, executor)
        .await?
    {
        sources.add(DocumentType::ReceiptReversal, row.base.id, Some(sale), None);
    }
    for row in db
        .payment_reversals()
        .find_many(doc! { "original_supplier_payment_id": { "$in": &payments } }, executor)
        .await?
    {
        sources.add(DocumentType::PaymentReversal, row.base.id, Some(sale), None);
    }
    Ok(())
}

async fn receivable(
    db: &Database,
    sale: &str,
    sources: &mut Sources,
    executor: &mut dyn Executor,
) -> Result<()> {
    let accounts = db.receivable_accounts().find_many(doc! { "sales_order_id": sale }, executor).await?;
    let accounts = accounts.into_iter().map(|row| row.base.id).collect::<Vec<_>>();
    let entries = db
        .receivable_entries()
        .find_many(doc! { "receivable_account_id": { "$in": &accounts } }, executor)
        .await?;
    let entries = entries.into_iter().map(|row| row.base.id).collect::<Vec<_>>();
    for row in db
        .customer_receipts()
        .find_many(doc! { "pending_allocations.receivable_entry_id": { "$in": &entries } }, executor)
        .await?
    {
        sources.add(DocumentType::CustomerReceipt, row.base.id, Some(sale), None);
    }
    for row in db
        .receipt_allocations()
        .find_many(doc! { "receivable_entry_id": { "$in": &entries } }, executor)
        .await?
    {
        let id = row.customer_receipt_id.to_string();
        sources.add(DocumentType::CustomerReceipt, id.clone(), Some(sale), None);
        allocation(
            sources,
            &id,
            &row.base.id,
            row.base.created_at,
            row.allocated_amount.to_string(),
            row.allocation_action.as_str(),
            "回款",
            TimelineCategory::Receivable,
        );
    }
    receivable_invoices(db, sale, sources, &accounts, executor).await?;
    customer_refunds(db, sale, sources, &entries, executor).await?;
    Ok(())
}

async fn payable(
    db: &Database,
    sale: &str,
    sources: &mut Sources,
    executor: &mut dyn Executor,
) -> Result<()> {
    let purchases = sources.ids(DocumentType::PurchaseOrder);
    for snapshot in db.purchase_payment_request_snapshots().find_for_purchases(&purchases, executor).await? {
        sources.add(DocumentType::PurchasePaymentRequest, snapshot.request_id, Some(sale), None);
    }
    let accounts = db
        .payable_accounts()
        .find_many(
            doc! { "source_type": "purchase_order", "source_document_id": { "$in": &purchases } },
            executor,
        )
        .await?;
    let accounts = accounts.into_iter().map(|row| row.base.id).collect::<Vec<_>>();
    let entries =
        db.payable_entries().find_many(doc! { "payable_account_id": { "$in": &accounts } }, executor).await?;
    let entries = entries.into_iter().map(|row| row.base.id).collect::<Vec<_>>();
    for row in
        db.payment_allocations().find_many(doc! { "payable_entry_id": { "$in": &entries } }, executor).await?
    {
        let id = row.supplier_payment_id.to_string();
        sources.add(DocumentType::SupplierPayment, id.clone(), Some(sale), None);
        allocation(
            sources,
            &id,
            &row.base.id,
            row.base.created_at,
            row.allocated_amount.to_string(),
            row.allocation_action.as_str(),
            "付款",
            TimelineCategory::Payable,
        );
    }
    payable_documents(db, sale, sources, &accounts, &purchases, executor).await?;
    supplier_refunds(db, sale, sources, &entries, executor).await?;
    Ok(())
}

async fn receivable_invoices(
    db: &Database,
    sale: &str,
    sources: &mut Sources,
    accounts: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    for row in db
        .sales_invoice_allocations()
        .find_many(doc! { "receivable_account_id": { "$in": &accounts } }, executor)
        .await?
    {
        let id = row.invoice_id.to_string();
        sources.add(DocumentType::Invoice, id.clone(), Some(sale), None);
        allocation(
            sources,
            &id,
            &row.base.id,
            row.base.created_at,
            row.allocated_gross_amount.to_string(),
            row.allocation_action.as_str(),
            "销项发票",
            TimelineCategory::Receivable,
        );
    }
    for row in db.sales_invoice_requests().find_many(doc! { "sales_order_id": sale }, executor).await? {
        sources.add(DocumentType::SalesInvoiceRequest, row.base.id, Some(sale), None);
    }
    Ok(())
}

async fn customer_refunds(
    db: &Database,
    sale: &str,
    sources: &mut Sources,
    entries: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    let receipts = sources.ids(DocumentType::CustomerReceipt);
    for row in db
        .customer_refunds()
        .find_many(
            doc! { "$or": [
                { "original_receipt_id": { "$in": receipts } },
                { "original_receivable_entry_id": { "$in": entries } },
                { "sales_return_case_id": { "$in": sources.ids(DocumentType::SalesReturnCase) } }
            ] },
            executor,
        )
        .await?
    {
        sources.add(DocumentType::CustomerRefund, row.base.id, Some(sale), None);
    }
    Ok(())
}

async fn payable_documents(
    db: &Database,
    sale: &str,
    sources: &mut Sources,
    accounts: &[String],
    purchases: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    for row in db
        .purchase_invoice_allocations()
        .find_many(doc! { "payable_account_id": { "$in": &accounts } }, executor)
        .await?
    {
        let id = row.invoice_id.to_string();
        sources.add(DocumentType::Invoice, id.clone(), Some(sale), None);
        allocation(
            sources,
            &id,
            &row.base.id,
            row.base.created_at,
            row.allocated_gross_amount.to_string(),
            row.allocation_action.as_str(),
            "进项发票",
            TimelineCategory::Payable,
        );
    }
    for row in db
        .purchase_payment_requests()
        .find_many(
            doc! { "$or": [
                { "lines.purchase_order_id": { "$in": purchases } },
                { "data.lines.payable_account_id": { "$in": accounts } }
            ] },
            executor,
        )
        .await?
    {
        sources.add(DocumentType::PurchasePaymentRequest, row.base.id, Some(sale), None);
    }
    Ok(())
}

async fn supplier_refunds(
    db: &Database,
    sale: &str,
    sources: &mut Sources,
    entries: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    let payments = sources.ids(DocumentType::SupplierPayment);
    for row in db
        .supplier_refunds()
        .find_many(
            doc! { "$or": [
                { "original_payment_id": { "$in": payments } },
                { "source_purchase_order_id": { "$in": sources.ids(DocumentType::PurchaseOrder) } },
                { "original_payable_entry_id": { "$in": entries } },
                { "purchase_return_order_id": { "$in": sources.ids(DocumentType::PurchaseReturnOrder) } }
            ] },
            executor,
        )
        .await?
    {
        sources.add(DocumentType::SupplierRefund, row.base.id, Some(sale), None);
    }
    Ok(())
}
