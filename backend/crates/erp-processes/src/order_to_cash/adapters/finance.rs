//! Financial facts adapted to the sales progress contract.

use async_trait::async_trait;
use erp_core::ids::SalesOrderId;
use erp_finance::repository::ReceivableExt;
use erp_sales::ports::sales_order::{ReceivableBalanceFact, SalesMoneyProgressPort};
use mongodb::Database;
use persistence_core::Executor;

/// Read finance facts only when the sales service requests them within its execution order.
pub struct FinanceMoneyProgressAdapter {
    db: Database,
}
impl FinanceMoneyProgressAdapter {
    /// Bind the provider database without performing any reads.
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
#[async_trait]
impl SalesMoneyProgressPort for FinanceMoneyProgressAdapter {
    async fn receivable_balances(
        &self,
        id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> erp_sales::Result<Vec<ReceivableBalanceFact>> {
        Ok(self
            .db
            .receivable_accounts()
            .money_progress_facts(id, executor)
            .await?
            .into_iter()
            .map(|fact| ReceivableBalanceFact {
                open_total: fact.open_total,
                settled_total: fact.settled_total,
                open_invoiceable_total: fact.open_invoiceable_total,
                invoiced_total: fact.invoiced_total,
            })
            .collect())
    }
}
