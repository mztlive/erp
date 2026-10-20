//! 退款、冲正沿原始资金和退货依据回到销售单，不按往来主体猜测来源。
use erp_finance::entity::payable::PayableSourceType;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_returns::repository::ReturnsExt;
use erp_workflow::DocumentType;
use persistence_core::Executor;

use super::TimelineRepository;
use crate::Result;

impl TimelineRepository<'_> {
    pub(super) async fn adjustment_sources(
        &self,
        kind: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(DocumentType, String)>> {
        use DocumentType::*;
        let mut sources = Vec::new();
        match kind {
            ReceiptReversal => {
                if let Some(row) = self.db.receipt_reversals().find_by_id(id, executor).await? {
                    sources.push((CustomerReceipt, row.original_customer_receipt_id.to_string()));
                }
            },
            PaymentReversal => {
                if let Some(row) = self.db.payment_reversals().find_by_id(id, executor).await? {
                    sources.push((SupplierPayment, row.original_supplier_payment_id.to_string()));
                }
            },
            CustomerRefund => sources.extend(self.customer_refund_sources(id, executor).await?),
            SupplierRefund => sources.extend(self.supplier_refund_sources(id, executor).await?),
            _ => {},
        }
        Ok(sources)
    }

    async fn customer_refund_sources(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(DocumentType, String)>> {
        let mut sources = Vec::new();
        let Some(row) = self.db.customer_refunds().find_by_id(id, executor).await? else {
            return Ok(sources);
        };
        if let Some(id) = row.original_receipt_id {
            sources.push((DocumentType::CustomerReceipt, id.to_string()));
        }
        if let Some(id) = row.sales_return_case_id {
            sources.push((DocumentType::SalesReturnCase, id.to_string()));
        }
        if let Some(id) = row.original_receivable_entry_id
            && let Some(entry) = self.db.receivable_entries().find_by_id(id.as_ref(), executor).await?
            && let Some(account) = self
                .db
                .receivable_accounts()
                .find_by_id(entry.receivable_account_id.as_ref(), executor)
                .await?
        {
            sources.push((DocumentType::SalesOrder, account.sales_order_id.to_string()));
        }
        Ok(sources)
    }

    async fn supplier_refund_sources(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(DocumentType, String)>> {
        let mut sources = Vec::new();
        let Some(row) = self.db.supplier_refunds().find_by_id(id, executor).await? else {
            return Ok(sources);
        };
        if let Some(id) = row.original_payment_id {
            sources.push((DocumentType::SupplierPayment, id.to_string()));
        }
        if let Some(id) = row.purchase_return_order_id {
            sources.push((DocumentType::PurchaseReturnOrder, id.to_string()));
        }
        if let Some(id) = row.source_purchase_order_id {
            sources.push((DocumentType::PurchaseOrder, id));
        }
        if let Some(id) = row.original_payable_entry_id
            && let Some(entry) = self.db.payable_entries().find_by_id(id.as_ref(), executor).await?
            && let Some(account) =
                self.db.payable_accounts().find_by_id(entry.payable_account_id.as_ref(), executor).await?
            && account.source_type == PayableSourceType::PurchaseOrder
        {
            sources.push((DocumentType::PurchaseOrder, account.source_document_id));
        }
        Ok(sources)
    }
}
