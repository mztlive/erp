//! Remaining-domain party names, creators and origin counterparties.

use std::collections::{HashMap, HashSet};

use database::PurchaseOrderExt;
use erp_audit::AuditExt;
use erp_customer::CustomerExt;
use erp_finance::entity::payable::PayableAccount;
use erp_finance::entity::receivable::ReceivableAccount;
use erp_finance::repository::PayableExt;
use erp_finance::repository::ReceivableExt;
use erp_party::Party;
use erp_party::PartyExt;
use erp_sales::repository::SalesOrderExt;
use erp_supplier::SupplierExt;
use persistence_core::Executor;

use super::super::amount::non_empty;
use crate::errors::Result;

impl crate::work_item::ProcessObjectFacts {
    /// Recover document creators from create-audit facts.
    pub(super) async fn load_created_by_from_audit(
        &self,
        resource_type: &str,
        ids: &HashSet<String>,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let resource_ids = ids.iter().cloned().collect::<Vec<_>>();
        let audits = self
            .db
            .audit_logs()
            .list_work_item_creation_audits(resource_type, &resource_ids, executor)
            .await?;
        let mut created_by = HashMap::new();
        for audit in audits {
            if let Some(resource_id) = audit.resource_id.as_deref() {
                created_by
                    .entry(resource_id.to_string())
                    .or_insert_with(|| audit.actor_id.clone());
            }
        }
        Ok(created_by)
    }

    /// Load payable supplier display names.
    pub(super) async fn payable_supplier_names(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let ids = accounts
            .iter()
            .map(|account| account.supplier_id.to_string())
            .collect::<Vec<_>>();
        self.supplier_display_names(&ids, executor).await
    }

    /// Load payable source purchase-order numbers.
    pub(super) async fn payable_purchase_numbers(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let ids = accounts
            .iter()
            .map(|account| account.source_document_id.clone())
            .collect::<Vec<_>>();
        Ok(self
            .db
            .purchase_orders()
            .list_active_by_ids(&ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.purchase_no))
            .collect())
    }

    /// Load current-revision legal names for parties.
    pub(super) async fn party_legal_names(
        &self,
        party_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if party_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let parties = self.db.parties().list_active_by_ids(party_ids, executor).await?;
        self.legal_names_for_parties(&parties, executor).await
    }

    async fn legal_names_for_parties(
        &self,
        parties: &[Party],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let revision_ids = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect::<Vec<_>>();
        if revision_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let names_by_revision = self
            .db
            .party_revisions()
            .list_active_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision.legal_name))
            .collect::<HashMap<_, _>>();
        Ok(parties
            .iter()
            .filter_map(|party| {
                let revision_id = party.stable.current_revision_id.as_ref()?;
                let name = names_by_revision.get(revision_id).cloned()?;
                non_empty(&name).map(|name| (party.base.id.clone(), name))
            })
            .collect())
    }

    /// Resolve customer display names from current party legal names, falling back to customer no.
    pub(super) async fn customer_display_names(
        &self,
        customer_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if customer_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let customers = self
            .db
            .customer_accounts()
            .list_active_by_ids(customer_ids, executor)
            .await?;
        let party_ids = customers
            .iter()
            .map(|item| item.party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(customers
            .into_iter()
            .map(|customer| {
                let name = party_names
                    .get(&customer.party_id.to_string())
                    .cloned()
                    .unwrap_or(customer.customer_no);
                (customer.base.id, name)
            })
            .collect())
    }

    /// Resolve supplier display names from current party legal names, falling back to supplier no.
    pub(in crate::work_item) async fn supplier_display_names(
        &self,
        supplier_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if supplier_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let suppliers = self
            .db
            .supplier_accounts()
            .list_active_by_ids(supplier_ids, executor)
            .await?;
        let party_ids = suppliers
            .iter()
            .map(|item| item.party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(suppliers
            .into_iter()
            .map(|supplier| {
                let name = party_names
                    .get(&supplier.party_id.to_string())
                    .cloned()
                    .unwrap_or(supplier.supplier_no);
                (supplier.base.id, name)
            })
            .collect())
    }

    /// Identify receivable source revisions that are voucher sales.
    pub(super) async fn receivable_voucher_revision_ids(
        &self,
        accounts: &[ReceivableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        let revision_ids = accounts
            .iter()
            .map(|account| account.source_sales_order_revision_id.to_string())
            .collect::<Vec<_>>();
        if revision_ids.is_empty() {
            return Ok(HashSet::new());
        }
        Ok(self
            .db
            .sales_order_revisions()
            .list_active_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .filter(|revision| {
                revision.voucher_category_sku_id.is_some() || revision.voucher_expiry_at.is_some()
            })
            .map(|revision| revision.base.id)
            .collect())
    }

    /// Load original receipt counterparties for refunds and reversals.
    pub(super) async fn customer_receipt_origins(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let receipts = self
            .db
            .customer_receipts()
            .list_active_by_ids(receipt_ids, executor)
            .await?;
        let party_ids = receipts
            .iter()
            .map(|receipt| receipt.counterparty_party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(receipts
            .into_iter()
            .filter_map(|receipt| {
                party_names
                    .get(&receipt.counterparty_party_id.to_string())
                    .cloned()
                    .map(|name| (receipt.base.id, name))
            })
            .collect())
    }

    /// Load original receivable-entry counterparties.
    pub(super) async fn receivable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let entries = self
            .db
            .receivable_entries()
            .list_active_by_ids(entry_ids, executor)
            .await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.receivable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .receivable_accounts()
            .list_active_by_ids(&account_ids, executor)
            .await?;
        let party_names = self
            .party_legal_names(
                &accounts
                    .iter()
                    .map(|account| account.counterparty_party_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let accounts = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account))
            .collect::<HashMap<_, _>>();
        Ok(entries
            .into_iter()
            .filter_map(|entry| {
                let account = accounts.get(&entry.receivable_account_id.to_string())?;
                party_names
                    .get(&account.counterparty_party_id.to_string())
                    .cloned()
                    .map(|name| (entry.base.id, name))
            })
            .collect())
    }

    /// Load original payment counterparties for refunds and reversals.
    pub(super) async fn supplier_payment_origins(
        &self,
        payment_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let payments = self
            .db
            .supplier_payments()
            .list_active_by_ids(payment_ids, executor)
            .await?;
        let supplier_names = self
            .supplier_display_names(
                &payments
                    .iter()
                    .map(|payment| payment.supplier_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        Ok(payments
            .into_iter()
            .filter_map(|payment| {
                supplier_names
                    .get(&payment.supplier_id.to_string())
                    .cloned()
                    .map(|name| (payment.base.id, name))
            })
            .collect())
    }

    /// Load original payable-entry counterparties.
    pub(super) async fn payable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let entries = self
            .db
            .payable_entries()
            .list_active_by_ids(entry_ids, executor)
            .await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.payable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .payable_accounts()
            .list_active_by_ids(&account_ids, executor)
            .await?;
        let supplier_names = self.payable_supplier_names(&accounts, executor).await?;
        let accounts = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account))
            .collect::<HashMap<_, _>>();
        Ok(entries
            .into_iter()
            .filter_map(|entry| {
                let account = accounts.get(&entry.payable_account_id.to_string())?;
                supplier_names
                    .get(&account.supplier_id.to_string())
                    .cloned()
                    .map(|name| (entry.base.id, name))
            })
            .collect())
    }
}
