//! Remaining-domain party names, creators and origin counterparties.

use std::collections::{HashMap, HashSet};

use erp_audit::AuditExt;
use erp_audit::repository::prelude::*;
use erp_customer::CustomerExt;
use erp_finance::entity::payable::PayableAccount;
use erp_finance::entity::receivable::ReceivableAccount;
use erp_party::{Party, PartyExt};
use erp_supplier::SupplierExt;
use persistence_core::Executor;

use super::super::amount::non_empty;
use crate::errors::Result;

impl super::super::WorkItemFactsReader {
    /// Recover document creators from create-audit facts.
    pub(in crate::workbench) async fn load_created_by_from_audit(
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
        Ok(first_created_by(
            audits.iter().map(|audit| (audit.resource_id.as_deref(), audit.actor_id.as_str())),
        ))
    }

    /// Load payable supplier display names.
    pub(in crate::workbench) async fn payable_supplier_names(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let ids = accounts.iter().map(|account| account.supplier_id.to_string()).collect::<Vec<_>>();
        self.supplier_display_names(&ids, executor).await
    }

    /// Load payable source purchase-order numbers.
    pub(in crate::workbench) async fn payable_purchase_numbers(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let ids = accounts.iter().map(|account| account.source_document_id.clone()).collect::<Vec<_>>();
        Ok(self
            .read_purchase_orders(&ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.purchase_no))
            .collect())
    }

    /// Load current-revision legal names for parties.
    pub(in crate::workbench) async fn party_legal_names(
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

    /// 读取本批主体当前修订的法定名称。
    ///
    /// # 参数
    /// * `parties` - 本批主体
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回主体 ID 到法定名称。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn legal_names_for_parties(
        &self,
        parties: &[Party],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let revision_ids =
            parties.iter().filter_map(|party| party.stable.current_revision_id.clone()).collect::<Vec<_>>();
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
    pub(in crate::workbench) async fn customer_display_names(
        &self,
        customer_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if customer_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let customers = self.db.customer_accounts().list_active_by_ids(customer_ids, executor).await?;
        let party_ids = customers.iter().map(|item| item.party_id.to_string()).collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(customers
            .into_iter()
            .map(|customer| {
                let name =
                    party_names.get(&customer.party_id.to_string()).cloned().unwrap_or(customer.customer_no);
                (customer.base.id, name)
            })
            .collect())
    }

    /// Resolve supplier display names from current party legal names, falling back to supplier no.
    pub(in crate::workbench) async fn supplier_display_names(
        &self,
        supplier_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if supplier_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let suppliers = self.db.supplier_accounts().list_active_by_ids(supplier_ids, executor).await?;
        let party_ids = suppliers.iter().map(|item| item.party_id.to_string()).collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(suppliers
            .into_iter()
            .map(|supplier| {
                let name =
                    party_names.get(&supplier.party_id.to_string()).cloned().unwrap_or(supplier.supplier_no);
                (supplier.base.id, name)
            })
            .collect())
    }

    /// Identify receivable source revisions that are voucher sales.
    pub(in crate::workbench) async fn receivable_voucher_revision_ids(
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
        let revisions = self.read_sales_revisions(&revision_ids, executor).await?;
        Ok(super::mapping::voucher_revision_ids(&revisions))
    }
}

/// 仅使用仓储既有返回顺序，每个 resource 保留首个 create actor。
fn first_created_by<'a>(
    audits: impl IntoIterator<Item = (Option<&'a str>, &'a str)>,
) -> HashMap<String, String> {
    let mut created_by = HashMap::new();
    for (resource_id, actor_id) in audits {
        if let Some(resource_id) = resource_id {
            created_by.entry(resource_id.to_string()).or_insert_with(|| actor_id.to_string());
        }
    }
    created_by
}
#[cfg(test)]
mod tests {
    #[test]
    fn first_created_by_keeps_first_actor_and_ignores_missing_resource() {
        let result = super::first_created_by([
            (Some("r-1"), "first"),
            (None, "orphan"),
            (Some("r-1"), "last"),
            (Some("r-2"), ""),
        ]);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("r-1").map(String::as_str), Some("first"));
        assert_eq!(result.get("r-2").map(String::as_str), Some(""));
    }
}
