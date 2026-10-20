use std::collections::BTreeSet;

use erp_finance::repository::{
    PayableExt, PurchasePaymentRequestExt, PurchasePaymentRequestSnapshotExt, ReceivableExt,
};
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_returns::repository::ReturnsExt;
use erp_sales::repository::SalesReviewExt;
use erp_workflow::DocumentType;
use mongodb::bson::doc;
use persistence_core::Executor;

use super::TimelineRepository;
use crate::Result;
use crate::sales_timeline::TimelineDocument;

impl TimelineRepository<'_> {
    pub(in crate::sales_timeline) async fn roots(
        &self,
        document: &mut TimelineDocument,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        use DocumentType::*;
        let kind = document.kind;
        let id = document.id.as_str();
        let mut roots = BTreeSet::new();
        let mut purchases = Vec::new();
        match kind {
            SalesInvoiceRequest => {
                if let Some(row) = self.db.sales_invoice_requests().find_by_id(id, executor).await? {
                    roots.insert(row.sales_order_id.to_string());
                }
            },
            PurchasePaymentRequest => {
                if let Some(row) = self.db.purchase_payment_requests().find_by_id(id, executor).await? {
                    purchases.extend(row.lines.into_iter().map(|line| line.purchase_order_id));
                }
                for snapshot in
                    self.db.purchase_payment_request_snapshots().find_for_request(id, executor).await?
                {
                    purchases.extend(snapshot.request.lines.into_iter().map(|line| line.purchase_order_id));
                }
            },
            ReceiptReversal | PaymentReversal | CustomerRefund | SupplierRefund => {
                for (source_kind, source_id) in self.adjustment_sources(kind, id, executor).await? {
                    let mut source = TimelineDocument::new(source_kind, source_id);
                    roots.extend(Box::pin(self.roots(&mut source, executor)).await?);
                }
            },
            _ => self.document_roots(document, &mut roots, &mut purchases, executor).await?,
        }
        for row in self.db.purchase_orders().find_many(doc! { "id": { "$in": purchases } }, executor).await? {
            // 只回填当前单据实际关联的采购来源；多来源资金单不得任选一个销售单。
            document.resolve_purchase_source(&row.base.id, row.sales_order_id.as_ref());
            roots.insert(row.sales_order_id.to_string());
        }
        Ok(roots)
    }

    async fn finance_roots(
        &self,
        kind: DocumentType,
        id: &str,
        roots: &mut BTreeSet<String>,
        purchases: &mut Vec<String>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut receivables = Vec::new();
        let mut payables = Vec::new();
        match kind {
            DocumentType::CustomerReceipt => receivables.extend(self.receipt_accounts(id, executor).await?),
            DocumentType::SupplierPayment => payables.extend(self.payment_accounts(id, executor).await?),
            DocumentType::Invoice => {
                receivables.extend(
                    self.db
                        .sales_invoice_allocations()
                        .find_many(doc! { "invoice_id": id }, executor)
                        .await?
                        .into_iter()
                        .map(|row| row.receivable_account_id.to_string()),
                );
                payables.extend(
                    self.db
                        .purchase_invoice_allocations()
                        .find_many(doc! { "invoice_id": id }, executor)
                        .await?
                        .into_iter()
                        .map(|row| row.payable_account_id.to_string()),
                );
            },
            _ => {},
        }
        for row in
            self.db.receivable_accounts().find_many(doc! { "id": { "$in": receivables } }, executor).await?
        {
            roots.insert(row.sales_order_id.to_string());
        }
        for row in self
            .db
            .payable_accounts()
            .find_many(doc! { "id": { "$in": payables }, "source_type": "purchase_order" }, executor)
            .await?
        {
            purchases.push(row.source_document_id);
        }
        Ok(())
    }
    async fn document_roots(
        &self,
        document: &mut TimelineDocument,
        roots: &mut BTreeSet<String>,
        purchases: &mut Vec<String>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use DocumentType::*;
        let id = document.id.as_str();
        match document.kind {
            SalesOrder | VoucherSalesOrder => {
                roots.insert(id.into());
            },
            SalesChangeOrder => {
                if let Some(row) = self.db.sales_change_orders().find_by_id(id, executor).await? {
                    roots.insert(row.sales_order_id.to_string());
                }
            },
            PurchaseOrder => purchases.push(id.into()),
            SalesReturnCase => {
                if let Some(row) = self.db.sales_return_cases().find_by_id(id, executor).await? {
                    roots.insert(row.sales_order_id.to_string());
                }
            },
            PurchaseReturnOrder => {
                if let Some(row) = self.db.purchase_return_orders().find_by_id(id, executor).await? {
                    purchases.push(row.purchase_order_id.to_string());
                }
            },
            PurchaseChangeOrder => {
                if let Some(row) = self.db.purchase_change_orders().find_by_id(id, executor).await? {
                    purchases.push(row.purchase_order_id.to_string());
                }
            },
            _ => self.fulfillment_roots(document, roots, purchases, executor).await?,
        }
        Ok(())
    }
    async fn fulfillment_roots(
        &self,
        document: &mut TimelineDocument,
        roots: &mut BTreeSet<String>,
        purchases: &mut Vec<String>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use DocumentType::*;
        let id = document.id.as_str();
        match document.kind {
            Delivery => {
                if let Some(row) = self.db.deliveries().find_by_id(id, executor).await? {
                    document.sales_order_id = Some(row.sales_order_id.to_string());
                    document.purchase_order_id = row.purchase_order_id.map(|id| id.to_string());
                }
            },
            CustomerAcceptance => {
                if let Some(row) = self.db.customer_acceptances().find_by_id(id, executor).await? {
                    document.sales_order_id = Some(row.sales_order_id.to_string());
                }
            },
            ElectronicDelivery => {
                if let Some(row) = self.db.electronic_deliveries().find_by_id(id, executor).await? {
                    document.purchase_order_id = Some(row.purchase_order_id.to_string());
                }
            },
            ServiceFulfillment => {
                if let Some(row) = self.db.service_fulfillments().find_by_id(id, executor).await? {
                    document.purchase_order_id = Some(row.purchase_order_id.to_string());
                }
            },
            PurchaseReceipt => {
                if let Some(row) = self.db.purchase_receipts().find_by_id(id, executor).await? {
                    document.purchase_order_id = Some(row.purchase_order_id.to_string());
                }
            },
            _ => self.finance_roots(document.kind, id, roots, purchases, executor).await?,
        }
        roots.extend(document.sales_order_id.iter().cloned());
        purchases.extend(document.purchase_order_id.iter().cloned());
        Ok(())
    }
    async fn receipt_accounts(&self, id: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let rows =
            self.db.receipt_allocations().find_many(doc! { "customer_receipt_id": id }, executor).await?;
        let mut ids = rows.into_iter().map(|row| row.receivable_entry_id.to_string()).collect::<Vec<_>>();
        if let Some(receipt) = self.db.customer_receipts().find_by_id(id, executor).await? {
            ids.extend(
                receipt.pending_allocations.into_iter().map(|line| line.receivable_entry_id.to_string()),
            );
        }
        Ok(self
            .db
            .receivable_entries()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
            .into_iter()
            .map(|row| row.receivable_account_id.to_string())
            .collect())
    }
    async fn payment_accounts(&self, id: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let rows =
            self.db.payment_allocations().find_many(doc! { "supplier_payment_id": id }, executor).await?;
        let ids = rows.into_iter().map(|row| row.payable_entry_id.to_string()).collect::<Vec<_>>();
        Ok(self
            .db
            .payable_entries()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
            .into_iter()
            .map(|row| row.payable_account_id.to_string())
            .collect())
    }
}
