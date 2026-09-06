//! Remaining-domain object-fact loaders for workflow composition.
//!
//! Command/query services live in `erp-workflow` / `erp-read-models`. This
//! module only adapts unmigrated domain repositories into workflow ports.

mod amount;
mod change_order_brief;
mod facts;
mod fulfillment_operation_brief;
mod funds_document_brief;
mod inventory_settlement_brief;
mod procurement_brief;
mod purchase_review_brief;
mod sales_order_brief;

use mongodb::Database;

/// Remaining-domain object-fact loader used by the workflow composition adapter.
#[derive(Clone)]
pub struct ProcessObjectFacts {
    /// MongoDB handle shared with remaining domain repositories.
    pub db: Database,
}

impl ProcessObjectFacts {
    /// Bind remaining domain repositories used by work-item authorization facts.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Whether a customer or supplier counterparty is currently active.
    pub async fn counterparty_is_active(
        &self,
        kind: &str,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::errors::Result<bool> {
        Ok(self
            .counterparty_numbers(kind, &[id.to_string()], executor)
            .await?
            .contains_key(id))
    }

    /// Display numbers for counterparties of one kind.
    pub async fn counterparty_numbers(
        &self,
        kind: &str,
        ids: &[String],
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::errors::Result<std::collections::HashMap<String, String>> {
        use database::{CustomerExt, SupplierExt};
        use erp_core::ids::{CustomerAccountId, SupplierAccountId};
        let mut numbers = std::collections::HashMap::new();
        match kind {
            "supplier" => {
                for id in ids {
                    if let Some(supplier) = self
                        .db
                        .supplier_accounts()
                        .find_by_id(&SupplierAccountId::new(id), executor)
                        .await?
                    {
                        numbers.insert(id.clone(), supplier.supplier_no);
                    }
                }
            }
            "customer" => {
                for id in ids {
                    if let Some(customer) = self
                        .db
                        .customer_accounts()
                        .find_by_id(&CustomerAccountId::new(id), executor)
                        .await?
                    {
                        numbers.insert(id.clone(), customer.customer_no);
                    }
                }
            }
            _ => {}
        }
        Ok(numbers)
    }

    /// Actors that must stay separated from a reassignment candidate.
    pub async fn assignment_separation_actors(
        &self,
        item: &erp_workflow::entity::work_item::WorkItem,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::errors::Result<Vec<String>> {
        let _ = (item, executor);
        Ok(Vec::new())
    }
}

pub(crate) use facts::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, SubjectBrief};
