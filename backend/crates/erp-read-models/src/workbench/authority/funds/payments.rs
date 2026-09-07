//! Supplier payment, refund and payment-reversal object facts.

use std::collections::HashSet;

use persistence_core::Executor;

use super::super::object_ids;
use super::mapping;
use crate::errors::Result;
use erp_workflow::ports::{ObjectFactMap, ObjectKind};

impl super::super::WorkItemFactsReader {
    /// Load supplier-payment identity, creator, counterparty and impact.
    pub(in crate::workbench) async fn load_supplier_payment_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierPayment);
        if ids.is_empty() {
            return Ok(());
        }
        let payments = self.read_supplier_payments(&ids, executor).await?;
        let created_by = self
            .load_created_by_from_audit(
                "supplier_payment",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let supplier_ids = payments
            .iter()
            .map(|item| item.supplier_id.to_string())
            .collect::<Vec<_>>();
        let supplier_names = self.supplier_display_names(&supplier_ids, executor).await?;
        for payment in payments {
            let fact = mapping::supplier_payment_fact(
                &payment,
                created_by.get(&payment.base.id),
                supplier_names.get(&payment.supplier_id.to_string()).cloned(),
            );
            facts.insert((ObjectKind::SupplierPayment, payment.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load supplier-refund identity, creator, counterparty and impact.
    pub(in crate::workbench) async fn load_supplier_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self.read_supplier_refunds(&ids, executor).await?;
        let created_by = self
            .load_created_by_from_audit(
                "supplier_refund",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let supplier_ids = refunds
            .iter()
            .map(|item| item.supplier_id.to_string())
            .collect::<Vec<_>>();
        let supplier_names = self.supplier_display_names(&supplier_ids, executor).await?;
        let payment_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_payment_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let payment_origins = self.supplier_payment_origins(&payment_ids, executor).await?;
        let entry_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_payable_entry_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let entry_origins = self.payable_entry_origins(&entry_ids, executor).await?;
        for refund in refunds {
            let fact = mapping::supplier_refund_fact(
                &refund,
                created_by.get(&refund.base.id),
                supplier_names.get(&refund.supplier_id.to_string()).cloned(),
                &payment_origins,
                &entry_origins,
            );
            facts.insert((ObjectKind::SupplierRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load payment-reversal identity, creator, counterparty and impact.
    pub(in crate::workbench) async fn load_payment_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PaymentReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self.read_payment_reversals(&ids, executor).await?;
        let created_by = self
            .load_created_by_from_audit(
                "payment_reversal",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let payment_ids = reversals
            .iter()
            .map(|reversal| reversal.original_supplier_payment_id.to_string())
            .collect::<Vec<_>>();
        let origins = self.supplier_payment_origins(&payment_ids, executor).await?;
        for reversal in reversals {
            let fact = mapping::payment_reversal_fact(&reversal, created_by.get(&reversal.base.id), &origins);
            facts.insert((ObjectKind::PaymentReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }
}
