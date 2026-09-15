//! Customer receipt, refund and receipt-reversal object facts.

use std::collections::HashSet;

use erp_workflow::ports::{ObjectFactMap, ObjectKind};
use persistence_core::Executor;

use super::super::object_ids;
use super::mapping;
use crate::errors::Result;

impl super::super::WorkItemFactsReader {
    /// Load customer-receipt identity, creator, counterparty and impact.
    pub(in crate::workbench) async fn load_customer_receipt_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use erp_finance::repository::ReceivableExt;
        let request_ids = object_ids(keys, ObjectKind::SalesInvoiceRequest);
        if !request_ids.is_empty() {
            for request in self.db.sales_invoice_requests().list_active_by_ids(&request_ids, executor).await?
            {
                facts.insert(
                    (ObjectKind::SalesInvoiceRequest, request.base.id.clone()),
                    mapping::invoice_request_fact(&request),
                );
            }
        }
        let ids = object_ids(keys, ObjectKind::CustomerReceipt);
        if ids.is_empty() {
            return Ok(());
        }
        let receipts = self.read_customer_receipts(&ids, executor).await?;
        if receipts.is_empty() {
            return Ok(());
        }
        let created_by = self
            .load_created_by_from_audit(
                "customer_receipt",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let party_ids =
            receipts.iter().map(|item| item.counterparty_party_id.to_string()).collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        for receipt in receipts {
            let fact = mapping::customer_receipt_fact(
                &receipt,
                created_by.get(&receipt.base.id),
                party_names.get(&receipt.counterparty_party_id.to_string()).cloned(),
            );
            facts.insert((ObjectKind::CustomerReceipt, receipt.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load customer-refund identity, creator, counterparty and impact.
    pub(in crate::workbench) async fn load_customer_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::CustomerRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self.read_customer_refunds(&ids, executor).await?;
        let created_by = self
            .load_created_by_from_audit(
                "customer_refund",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let customer_ids = refunds.iter().map(|refund| refund.customer_id.to_string()).collect::<Vec<_>>();
        let customer_names = self.customer_display_names(&customer_ids, executor).await?;
        let receipt_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_receipt_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let receipt_origins = self.customer_receipt_origins(&receipt_ids, executor).await?;
        let entry_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_receivable_entry_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let entry_origins = self.receivable_entry_origins(&entry_ids, executor).await?;
        for refund in refunds {
            let fact = mapping::customer_refund_fact(
                &refund,
                created_by.get(&refund.base.id),
                customer_names.get(&refund.customer_id.to_string()).cloned(),
                &receipt_origins,
                &entry_origins,
            );
            facts.insert((ObjectKind::CustomerRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load receipt-reversal identity, creator, counterparty and impact.
    pub(in crate::workbench) async fn load_receipt_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceiptReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self.read_receipt_reversals(&ids, executor).await?;
        let created_by = self
            .load_created_by_from_audit(
                "receipt_reversal",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let receipt_ids = reversals
            .iter()
            .map(|reversal| reversal.original_customer_receipt_id.to_string())
            .collect::<Vec<_>>();
        let origins = self.customer_receipt_origins(&receipt_ids, executor).await?;
        for reversal in reversals {
            let fact = mapping::receipt_reversal_fact(&reversal, created_by.get(&reversal.base.id), &origins);
            facts.insert((ObjectKind::ReceiptReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }
}
