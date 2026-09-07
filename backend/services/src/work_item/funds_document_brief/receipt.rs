//! Customer receipt, refund and receipt-reversal object facts.

use std::collections::HashSet;

use database::ReturnsExt;
use erp_finance::repository::ReceivableExt;
use persistence_core::Executor;

use super::super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind};
use crate::errors::Result;

impl crate::work_item::ProcessObjectFacts {
    /// Load customer-receipt identity, creator, counterparty and impact.
    pub(in crate::work_item) async fn load_customer_receipt_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::CustomerReceipt);
        if ids.is_empty() {
            return Ok(());
        }
        let receipts = self
            .db
            .customer_receipts()
            .list_active_by_ids(&ids, executor)
            .await?;
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
        let party_ids = receipts
            .iter()
            .map(|item| item.counterparty_party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        for receipt in receipts {
            let counterparty = party_names
                .get(&receipt.counterparty_party_id.to_string())
                .cloned();
            let mut fact = ObjectFact::new(
                receipt.base.id.clone(),
                format!("回款单 {}", receipt.receipt_no),
                created_by.get(&receipt.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = counterparty;
            fact.impact_summary = Some("不审批则回款不能过账、不能核销应收".to_string());
            facts.insert((ObjectKind::CustomerReceipt, receipt.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load customer-refund identity, creator, counterparty and impact.
    pub(in crate::work_item) async fn load_customer_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::CustomerRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self
            .db
            .customer_refunds()
            .list_active_by_ids(&ids, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "customer_refund",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let customer_ids = refunds
            .iter()
            .map(|refund| refund.customer_id.to_string())
            .collect::<Vec<_>>();
        let customer_names = self.customer_display_names(&customer_ids, executor).await?;
        let receipt_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_receipt_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let receipt_origins = self.customer_receipt_origins(&receipt_ids, executor).await?;
        let entry_ids = refunds
            .iter()
            .filter_map(|refund| {
                refund
                    .original_receivable_entry_id
                    .as_ref()
                    .map(ToString::to_string)
            })
            .collect::<Vec<_>>();
        let entry_origins = self.receivable_entry_origins(&entry_ids, executor).await?;
        for refund in refunds {
            let customer = customer_names.get(&refund.customer_id.to_string()).cloned();
            let origin = refund
                .original_receipt_id
                .as_ref()
                .and_then(|id| receipt_origins.get(&id.to_string()))
                .or_else(|| {
                    refund
                        .original_receivable_entry_id
                        .as_ref()
                        .and_then(|id| entry_origins.get(&id.to_string()))
                });
            let mut fact = ObjectFact::new(
                refund.base.id.clone(),
                format!("客户退款 {}", refund.refund_no),
                created_by.get(&refund.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = customer.or_else(|| origin.cloned());
            fact.impact_summary = Some("不审批则客户退款不能过账".to_string());
            facts.insert((ObjectKind::CustomerRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load receipt-reversal identity, creator, counterparty and impact.
    pub(in crate::work_item) async fn load_receipt_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceiptReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self
            .db
            .receipt_reversals()
            .list_active_by_ids(&ids, executor)
            .await?;
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
            let origin = origins.get(&reversal.original_customer_receipt_id.to_string());
            let mut fact = ObjectFact::new(
                reversal.base.id.clone(),
                format!("回款冲正 {}", reversal.reversal_no),
                created_by.get(&reversal.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = origin.cloned();
            fact.impact_summary = Some("不审批则回款冲正不能过账".to_string());
            facts.insert((ObjectKind::ReceiptReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }
}
