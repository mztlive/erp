//! 命令来源只返回存在名称的条目；富显示从同一已读行建立独立投影。
use crate::errors::Result;
use erp_finance::entity::{
    payable::{PayableAccount, PayableEntry, SupplierPayment},
    receivable::{CustomerReceipt, ReceivableAccount, ReceivableEntry},
};
use persistence_core::Executor;
use std::collections::HashMap;
impl super::super::WorkItemFactsReader {
    /// Load original receipt counterparties for refunds and reversals.
    pub(in crate::workbench) async fn customer_receipt_origins(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let receipts = self.read_customer_receipts(receipt_ids, executor).await?;
        let party_ids = receipts
            .iter()
            .map(|receipt| receipt.counterparty_party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(receipt_counterparties(&receipts, &party_names))
    }
    /// Load original receivable-entry counterparties.
    pub(in crate::workbench) async fn receivable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let entries = self.read_receivable_entries(entry_ids, executor).await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.receivable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self.read_receivable_accounts(&account_ids, executor).await?;
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
        Ok(receivable_entry_counterparties(&entries, &accounts, &party_names))
    }
    /// Load original payment counterparties for refunds and reversals.
    pub(in crate::workbench) async fn supplier_payment_origins(
        &self,
        payment_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let payments = self.read_supplier_payments(payment_ids, executor).await?;
        let supplier_names = self
            .supplier_display_names(
                &payments
                    .iter()
                    .map(|payment| payment.supplier_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        Ok(payment_counterparties(&payments, &supplier_names))
    }
    /// Load original payable-entry counterparties.
    pub(in crate::workbench) async fn payable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let entries = self.read_payable_entries(entry_ids, executor).await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.payable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self.read_payable_accounts(&account_ids, executor).await?;
        let supplier_names = self.payable_supplier_names(&accounts, executor).await?;
        let accounts = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account))
            .collect::<HashMap<_, _>>();
        Ok(payable_entry_counterparties(&entries, &accounts, &supplier_names))
    }
}
/// 将已读来源投影成命令名称；缺名称或账户的来源不入 map。
pub(in crate::workbench) fn receipt_counterparties(
    receipts: &[CustomerReceipt],
    party_names: &HashMap<String, String>,
) -> HashMap<String, String> {
    receipts
        .iter()
        .filter_map(|receipt| {
            party_names
                .get(&receipt.counterparty_party_id.to_string())
                .cloned()
                .map(|name| (receipt.base.id.clone(), name))
        })
        .collect()
}

/// 将已读来源投影成命令名称；缺名称或账户的来源不入 map。
pub(in crate::workbench) fn receivable_entry_counterparties(
    entries: &[ReceivableEntry],
    accounts: &HashMap<String, ReceivableAccount>,
    party_names: &HashMap<String, String>,
) -> HashMap<String, String> {
    entries
        .iter()
        .filter_map(|entry| {
            let account = accounts.get(&entry.receivable_account_id.to_string())?;
            party_names
                .get(&account.counterparty_party_id.to_string())
                .cloned()
                .map(|name| (entry.base.id.clone(), name))
        })
        .collect()
}

/// 将已读来源投影成命令名称；缺名称或账户的来源不入 map。
pub(in crate::workbench) fn payment_counterparties(
    payments: &[SupplierPayment],
    supplier_names: &HashMap<String, String>,
) -> HashMap<String, String> {
    payments
        .iter()
        .filter_map(|payment| {
            supplier_names
                .get(&payment.supplier_id.to_string())
                .cloned()
                .map(|name| (payment.base.id.clone(), name))
        })
        .collect()
}

/// 将已读来源投影成命令名称；缺名称或账户的来源不入 map。
pub(in crate::workbench) fn payable_entry_counterparties(
    entries: &[PayableEntry],
    accounts: &HashMap<String, PayableAccount>,
    supplier_names: &HashMap<String, String>,
) -> HashMap<String, String> {
    entries
        .iter()
        .filter_map(|entry| {
            let account = accounts.get(&entry.payable_account_id.to_string())?;
            supplier_names
                .get(&account.supplier_id.to_string())
                .cloned()
                .map(|name| (entry.base.id.clone(), name))
        })
        .collect()
}
