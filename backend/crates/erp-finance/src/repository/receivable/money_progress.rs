//! Publish the existing account selection as narrow balance facts.

use crate::entity::receivable::money_progress_facts::ReceivableMoneyProgressFact;
use crate::repository::owned::ReceivableAccountRepository;
use erp_core::ids::SalesOrderId;
use persistence_core::{Executor, Result};

impl ReceivableAccountRepository<'_> {
    /// Read per-account balances with the original account filtering, ordering and errors.
    ///
    /// This reuses the established repository query and the caller's executor. No aggregation,
    /// review-status filtering or additional transaction is introduced.
    pub async fn money_progress_facts(
        &self,
        id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableMoneyProgressFact>> {
        Ok(self
            .list_by_sales_order(id, executor)
            .await?
            .iter()
            .map(Into::into)
            .collect())
    }
}
