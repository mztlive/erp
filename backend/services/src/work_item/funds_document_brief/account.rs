//! Receivable and payable account object facts.

use std::collections::HashSet;

use database::{PayableExt, ReceivableExt};
use persistence_core::Executor;

use super::super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind};
use super::mapping::payable_account_fact;
use crate::errors::Result;

impl crate::work_item::ProcessObjectFacts {
    /// Load receivable-account identity, counterparty and impact.
    pub(in crate::work_item) async fn load_receivable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceivableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        let accounts = self
            .db
            .receivable_accounts()
            .list_active_by_ids(&ids, executor)
            .await?;
        if accounts.is_empty() {
            return Ok(());
        }
        let party_ids = accounts
            .iter()
            .map(|item| item.counterparty_party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        let voucher_revisions = self.receivable_voucher_revision_ids(&accounts, executor).await?;
        for account in accounts {
            let counterparty = party_names
                .get(&account.counterparty_party_id.to_string())
                .cloned();
            let is_voucher = voucher_revisions.contains(&account.source_sales_order_revision_id.to_string());
            let mut fact = ObjectFact::new(
                account.sales_order_id.to_string(),
                format!("应收子账 {}", account.account_seq),
                account.stable.created_by,
            );
            fact.counterparty_label = counterparty;
            fact.impact_summary = Some(if is_voucher {
                "不复核则卡券票款、开票与兑付前置事实不能确认".to_string()
            } else {
                "不复核则票款与开票事实不能确认".to_string()
            });
            facts.insert((ObjectKind::ReceivableAccount, account.base.id.clone()), fact);
        }
        Ok(())
    }

    /// Load payable-account identity, counterparty and unpaid impact.
    pub(in crate::work_item) async fn load_payable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PayableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        let accounts = self
            .db
            .payable_accounts()
            .list_active_by_ids(&ids, executor)
            .await?;
        let supplier_names = self.payable_supplier_names(&accounts, executor).await?;
        let purchase_nos = self.payable_purchase_numbers(&accounts, executor).await?;
        for account in accounts {
            let supplier = supplier_names.get(&account.supplier_id.to_string()).cloned();
            let purchase_no = purchase_nos.get(&account.source_document_id).cloned();
            let id = account.base.id.clone();
            facts.insert(
                (ObjectKind::PayableAccount, id),
                payable_account_fact(account, supplier, purchase_no),
            );
        }
        Ok(())
    }
}
