use erp_finance::entity::receivable::InvoiceDirection;
use erp_finance::repository::ReceivableExt;
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_returns::repository::ReturnsExt;
use erp_sales::repository::SalesReviewExt;
use erp_workflow::{DocumentRegistryExt, DocumentType};
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use super::super::TimelineCategory;
use super::super::sources::Sources;
use crate::Result;

pub(in crate::sales_timeline) async fn numbers(
    sources: &mut Sources,
    db: &Database,
    executor: &mut dyn Executor,
) -> Result<()> {
    let ids = sources.documents.keys().cloned().collect::<Vec<_>>();
    for row in db.business_documents().find_many(doc! { "id": { "$in": ids } }, executor).await? {
        if let Some(document) = sources.documents.get_mut(&row.base.id) {
            document.number = row.document_no;
        }
    }
    for row in db
        .invoices()
        .find_many(doc! { "id": { "$in": sources.ids(DocumentType::Invoice) } }, executor)
        .await?
    {
        if let Some(document) = sources.documents.get_mut(&row.base.id) {
            document.category_override = Some(match row.invoice_direction {
                InvoiceDirection::Sales => TimelineCategory::Receivable,
                InvoiceDirection::Purchase => TimelineCategory::Payable,
            });
            if document.number.is_empty() {
                document.number = row.invoice_no.unwrap_or_default();
            }
        }
    }
    super::submissions::append(db, sources, executor).await?;
    sources.check_size()?;
    Ok(())
}
pub(in crate::sales_timeline) async fn discover(
    db: &Database,
    sale: &str,
    executor: &mut dyn Executor,
) -> Result<Sources> {
    let mut sources = Sources::new();
    sources.add(DocumentType::SalesOrder, sale.into(), Some(sale), None);
    for row in db.sales_change_orders().find_many(doc! { "sales_order_id": sale }, executor).await? {
        sources.add(DocumentType::SalesChangeOrder, row.base.id, Some(sale), None);
    }
    for row in db.purchase_orders().find_many(doc! { "sales_order_id": sale }, executor).await? {
        let id = row.base.id;
        sources.add(DocumentType::PurchaseOrder, id.clone(), Some(sale), Some(&id));
    }
    fulfillment(db, sale, &mut sources, executor).await?;
    returns(db, sale, &mut sources, executor).await?;
    super::finance::discover(db, sale, &mut sources, executor).await?;
    sources.check_size()?;
    numbers(&mut sources, db, executor).await?;
    Ok(sources)
}

async fn fulfillment(
    db: &Database,
    sale: &str,
    sources: &mut Sources,
    executor: &mut dyn Executor,
) -> Result<()> {
    let purchases = sources.ids(DocumentType::PurchaseOrder);
    for row in db.deliveries().find_many(doc! { "sales_order_id": sale }, executor).await? {
        sources.add(DocumentType::Delivery, row.base.id, Some(sale), row.purchase_order_id.as_deref());
    }
    for row in db.customer_acceptances().find_many(doc! { "sales_order_id": sale }, executor).await? {
        sources.add(DocumentType::CustomerAcceptance, row.base.id, Some(sale), None);
    }
    for row in db
        .purchase_receipts()
        .find_many(doc! { "purchase_order_id": { "$in": &purchases } }, executor)
        .await?
    {
        sources.add(
            DocumentType::PurchaseReceipt,
            row.base.id,
            Some(sale),
            Some(row.purchase_order_id.as_ref()),
        );
    }
    for row in db
        .electronic_deliveries()
        .find_many(doc! { "purchase_order_id": { "$in": &purchases } }, executor)
        .await?
    {
        sources.add(
            DocumentType::ElectronicDelivery,
            row.base.id,
            Some(sale),
            Some(row.purchase_order_id.as_ref()),
        );
    }
    for row in db
        .service_fulfillments()
        .find_many(doc! { "purchase_order_id": { "$in": &purchases } }, executor)
        .await?
    {
        sources.add(
            DocumentType::ServiceFulfillment,
            row.base.id,
            Some(sale),
            Some(row.purchase_order_id.as_ref()),
        );
    }
    Ok(())
}

async fn returns(
    db: &Database,
    sale: &str,
    sources: &mut Sources,
    executor: &mut dyn Executor,
) -> Result<()> {
    let purchases = sources.ids(DocumentType::PurchaseOrder);
    for row in db
        .purchase_change_orders()
        .find_many(doc! { "purchase_order_id": { "$in": &purchases } }, executor)
        .await?
    {
        sources.add(
            DocumentType::PurchaseChangeOrder,
            row.base.id,
            Some(sale),
            Some(row.purchase_order_id.as_ref()),
        );
    }
    for row in db.sales_return_cases().find_many(doc! { "sales_order_id": sale }, executor).await? {
        sources.add(DocumentType::SalesReturnCase, row.base.id, Some(sale), None);
    }
    for row in db
        .purchase_return_orders()
        .find_many(doc! { "purchase_order_id": { "$in": &purchases } }, executor)
        .await?
    {
        sources.add(
            DocumentType::PurchaseReturnOrder,
            row.base.id,
            Some(sale),
            Some(row.purchase_order_id.as_ref()),
        );
    }
    Ok(())
}
