//! 命令来源只返回存在名称的条目；富显示从同一已读行建立独立投影。
use std::collections::HashMap;

use erp_finance::entity::payable::{PayableAccount, PayableEntry, SupplierPayment};
use erp_finance::entity::receivable::{CustomerReceipt, ReceivableAccount, ReceivableEntry};
use persistence_core::Executor;

use crate::errors::Result;
impl super::super::WorkItemFactsReader {
    /// 读取退款与冲正所引用的原回款往来名称。
    ///
    /// # 参数
    /// * `receipt_ids` - 原客户回款 ID。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回回款 ID 到主体法定名称。没有名称的回款不入映射。
    ///
    /// # 错误
    /// 回款或主体名称读取失败时返回错误。
    pub(in crate::workbench) async fn customer_receipt_origins(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let receipts = self.read_customer_receipts(receipt_ids, executor).await?;
        let party_ids =
            receipts.iter().map(|receipt| receipt.counterparty_party_id.to_string()).collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(receipt_counterparties(&receipts, &party_names))
    }
    /// 读取原应收分录经所属账户解析出的往来名称。
    ///
    /// # 参数
    /// * `entry_ids` - 应收分录 ID。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回分录 ID 到主体法定名称。账户或名称缺失的分录不入映射。
    ///
    /// # 错误
    /// 分录、账户或主体名称读取失败时返回错误。
    pub(in crate::workbench) async fn receivable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let entries = self.read_receivable_entries(entry_ids, executor).await?;
        let account_ids =
            entries.iter().map(|entry| entry.receivable_account_id.to_string()).collect::<Vec<_>>();
        let accounts = self.read_receivable_accounts(&account_ids, executor).await?;
        let party_names = self
            .party_legal_names(
                &accounts.iter().map(|account| account.counterparty_party_id.to_string()).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let accounts =
            accounts.into_iter().map(|account| (account.base.id.clone(), account)).collect::<HashMap<_, _>>();
        Ok(receivable_entry_counterparties(&entries, &accounts, &party_names))
    }
    /// 读取退款与冲正所引用的原付款往来名称。
    ///
    /// # 参数
    /// * `payment_ids` - 原供应商付款 ID。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回付款 ID 到供应商显示名。没有名称的付款不入映射。
    ///
    /// # 错误
    /// 付款或供应商名称读取失败时返回错误。
    pub(in crate::workbench) async fn supplier_payment_origins(
        &self,
        payment_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let payments = self.read_supplier_payments(payment_ids, executor).await?;
        let supplier_names = self
            .supplier_display_names(
                &payments.iter().map(|payment| payment.supplier_id.to_string()).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        Ok(payment_counterparties(&payments, &supplier_names))
    }
    /// 读取原应付分录经所属账户解析出的往来名称。
    ///
    /// # 参数
    /// * `entry_ids` - 应付分录 ID。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回分录 ID 到供应商显示名。账户或名称缺失的分录不入映射。
    ///
    /// # 错误
    /// 分录、账户或供应商名称读取失败时返回错误。
    pub(in crate::workbench) async fn payable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let entries = self.read_payable_entries(entry_ids, executor).await?;
        let account_ids =
            entries.iter().map(|entry| entry.payable_account_id.to_string()).collect::<Vec<_>>();
        let accounts = self.read_payable_accounts(&account_ids, executor).await?;
        let supplier_names = self.payable_supplier_names(&accounts, executor).await?;
        let accounts =
            accounts.into_iter().map(|account| (account.base.id.clone(), account)).collect::<HashMap<_, _>>();
        Ok(payable_entry_counterparties(&entries, &accounts, &supplier_names))
    }
}
/// 将已读来源投影成命令名称；缺名称或账户的来源不入 map。
///
/// # 参数
/// * `receipts` - 已读客户回款。
/// * `party_names` - 主体 ID 到法定名称。
///
/// # 返回
/// 返回回款 ID 到名称。主体没有名称的回款被丢弃。
///
/// # 错误
/// 不返回错误。
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
///
/// # 参数
/// * `entries` - 已读应收分录。
/// * `accounts` - 应收账户 ID 到账户。
/// * `party_names` - 主体 ID 到法定名称。
///
/// # 返回
/// 返回分录 ID 到名称。账户或主体名称缺失的分录被丢弃。
///
/// # 错误
/// 不返回错误。
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
///
/// # 参数
/// * `payments` - 已读供应商付款。
/// * `supplier_names` - 供应商 ID 到显示名。
///
/// # 返回
/// 返回付款 ID 到名称。没有显示名的付款被丢弃。
///
/// # 错误
/// 不返回错误。
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
///
/// # 参数
/// * `entries` - 已读应付分录。
/// * `accounts` - 应付账户 ID 到账户。
/// * `supplier_names` - 供应商 ID 到显示名。
///
/// # 返回
/// 返回分录 ID 到名称。账户或供应商名称缺失的分录被丢弃。
///
/// # 错误
/// 不返回错误。
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
