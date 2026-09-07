//! Facts required by sales order rules without depending on provider domains.

use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::SalesOrderId;
use erp_core::money::Amount;
use persistence_core::Executor;

/// One receivable account's balances; zero accounts must remain distinguishable from an empty list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceivableBalanceFact {
    /// Unsettled amount of this individual account.
    pub open_total: Amount,
    /// Settled amount of this individual account.
    pub settled_total: Amount,
    /// Remaining invoiceable amount of this individual account.
    pub open_invoiceable_total: Amount,
    /// Invoiced amount of this individual account.
    pub invoiced_total: Amount,
}

/// Supply balances within the caller's transaction after sales existence has been checked.
#[async_trait]
pub trait SalesMoneyProgressPort: Send + Sync {
    /// Read every undeleted account for the sales order, without aggregating away zero accounts.
    ///
    /// Provider failures propagate before any sales progress write.
    async fn receivable_balances(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<ReceivableBalanceFact>>;
}

/// Exact SKU and revision identities accepted by the current catalog.
#[async_trait]
pub trait SellableSkuPort: Send + Sync {
    /// Return qualified pairs for the requested business date using the caller's executor.
    ///
    /// Missing pairs remain absent; repository failures must propagate unchanged in class.
    async fn qualified_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<(String, String)>>;
}
