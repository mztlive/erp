//! 按已经裁剪的回款身份批量补齐名称，不扩大资金读取范围。

use std::collections::HashMap;

use erp_core::ids::CustomerAccountId;
use erp_customer::CustomerExt;
use erp_customer::repository::prelude::*;
use persistence_core::Executor;

use super::authorization::FundsAccess;
use super::rows::ScopedCustomerReceiptRow;
use crate::Result;

impl FundsAccess {
    /// 补齐已获整单授权的回款主体与经营客户名称。
    ///
    /// # 参数
    /// * `rows` - 当前页或详情的已裁剪行。
    /// * `executor` - 当前读取事务。
    /// # 返回
    /// 名称写回响应；缺失关联保持空值，不用另一种身份的名称替代。
    /// # 错误
    /// 客户或主体仓储读取失败时返回错误。
    pub(super) async fn finish_receipt_names(
        &self,
        rows: &mut [ScopedCustomerReceiptRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let party_ids = rows.iter().filter_map(|row| row.counterparty_party_id.clone()).collect::<Vec<_>>();
        let names = self.party_legal_names(&party_ids, executor).await?;
        let customer_ids =
            crate::support::dedup_sorted(rows.iter().filter_map(|row| row.customer_id.clone()));
        let mut customer_names = HashMap::new();
        if !customer_ids.is_empty() {
            let keys = customer_ids.into_iter().map(CustomerAccountId::new).collect::<Vec<_>>();
            let customers = self.db.customer_accounts().find_accounts_by_ids(&keys, executor).await?;
            let parties = customers.iter().map(|customer| customer.party_id.to_string()).collect::<Vec<_>>();
            let legal = self.party_legal_names(&parties, executor).await?;
            for customer in customers {
                if let Some(name) = legal.get(customer.party_id.as_ref()) {
                    customer_names.insert(customer.base.id, name.clone());
                }
            }
        }
        apply_receipt_names(rows, &names, &customer_names);
        Ok(())
    }
}

/// 按裁剪后仍可见的身份匹配名称，受限行不得取得名称。
///
/// # 参数
/// * `rows` - 已授权裁剪行。
/// * `parties` - 结算主体名称映射。
/// * `customers` - 经营客户名称映射。
/// # 返回
/// 原地更新两个独立身份的名称。
/// # 错误
/// 不返回错误。
pub(super) fn apply_receipt_names(
    rows: &mut [ScopedCustomerReceiptRow],
    parties: &HashMap<String, String>,
    customers: &HashMap<String, String>,
) {
    for row in rows {
        row.counterparty_party_name =
            row.counterparty_party_id.as_ref().and_then(|id| parties.get(id)).cloned();
        row.customer_name = row.customer_id.as_ref().and_then(|id| customers.get(id)).cloned();
    }
}
