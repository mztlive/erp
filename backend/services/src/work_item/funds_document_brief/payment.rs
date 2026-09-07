//! Supplier payment, refund and payment-reversal object facts.

use std::collections::HashSet;

use database::ReturnsExt;
use erp_finance::repository::PayableExt;
use persistence_core::Executor;

use super::super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind};
use crate::errors::Result;

impl crate::work_item::ProcessObjectFacts {
    /// Load supplier-payment identity, creator, counterparty and impact.
    pub(in crate::work_item) async fn load_supplier_payment_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierPayment);
        if ids.is_empty() {
            return Ok(());
        }
        let payments = self
            .db
            .supplier_payments()
            .list_active_by_ids(&ids, executor)
            .await?;
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
            let supplier = supplier_names.get(&payment.supplier_id.to_string()).cloned();
            let mut fact = ObjectFact::new(
                payment.base.id.clone(),
                format!("供应商付款 {}", payment.payment_no),
                created_by.get(&payment.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = supplier;
            fact.impact_summary = Some("付款已登记并过账；纠错须走付款冲正或供应商退款".to_string());
            facts.insert((ObjectKind::SupplierPayment, payment.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load supplier-refund identity, creator, counterparty and impact.
    pub(in crate::work_item) async fn load_supplier_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self
            .db
            .supplier_refunds()
            .list_active_by_ids(&ids, executor)
            .await?;
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
            let supplier = supplier_names.get(&refund.supplier_id.to_string()).cloned();
            let origin = refund
                .original_payment_id
                .as_ref()
                .and_then(|id| payment_origins.get(&id.to_string()))
                .or_else(|| {
                    refund
                        .original_payable_entry_id
                        .as_ref()
                        .and_then(|id| entry_origins.get(&id.to_string()))
                });
            let mut fact = ObjectFact::new(
                refund.base.id.clone(),
                format!("供应商退款 {}", refund.refund_no),
                created_by.get(&refund.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = supplier.or_else(|| origin.cloned());
            fact.impact_summary = Some("不审批则供应商退款不能过账".to_string());
            facts.insert((ObjectKind::SupplierRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load payment-reversal identity, creator, counterparty and impact.
    pub(in crate::work_item) async fn load_payment_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PaymentReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self
            .db
            .payment_reversals()
            .list_active_by_ids(&ids, executor)
            .await?;
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
            let origin = origins.get(&reversal.original_supplier_payment_id.to_string());
            let mut fact = ObjectFact::new(
                reversal.base.id.clone(),
                format!("付款冲正 {}", reversal.reversal_no),
                created_by.get(&reversal.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = origin.cloned();
            fact.impact_summary = Some("不审批则付款冲正不能过账".to_string());
            facts.insert((ObjectKind::PaymentReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }
}
