//! Receivable and payable account object facts.

use std::collections::HashSet;

use erp_workflow::ports::{ObjectFactMap, ObjectKind};
use persistence_core::Executor;

use super::super::object_ids;
use super::mapping;
use crate::errors::Result;

impl super::super::WorkItemFactsReader {
    /// Load receivable-account identity, counterparty and impact.
    pub(in crate::workbench) async fn load_receivable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceivableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        let accounts = self.read_receivable_accounts(&ids, executor).await?;
        if accounts.is_empty() {
            return Ok(());
        }
        let party_ids =
            accounts.iter().map(|item| item.counterparty_party_id.to_string()).collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        let voucher_revisions = self.receivable_voucher_revision_ids(&accounts, executor).await?;
        for account in accounts {
            let fact = mapping::receivable_account_fact(
                &account,
                party_names.get(&account.counterparty_party_id.to_string()).cloned(),
                voucher_revisions.contains(&account.source_sales_order_revision_id.to_string()),
            );
            facts.insert((ObjectKind::ReceivableAccount, account.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load payable-account identity, counterparty and unpaid impact.
    pub(in crate::workbench) async fn load_payable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PayableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        let accounts = self.read_payable_accounts(&ids, executor).await?;
        let supplier_names = self.payable_supplier_names(&accounts, executor).await?;
        let purchase_nos = self.payable_purchase_numbers(&accounts, executor).await?;
        for account in accounts {
            let fact = mapping::payable_account_fact(
                &account,
                supplier_names.get(&account.supplier_id.to_string()).cloned(),
                purchase_nos.get(&account.source_document_id).cloned(),
            );
            facts.insert((ObjectKind::PayableAccount, account.base.id.clone()), fact);
        }
        Ok(())
    }
}
