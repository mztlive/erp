//! 将财务事实适配为销售进度合同。

use async_trait::async_trait;
use erp_core::ids::SalesOrderId;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_sales::ports::sales_order::{ReceivableBalanceFact, SalesMoneyProgressPort};
use mongodb::Database;
use persistence_core::Executor;

/// Read finance facts only when the sales service requests them within its execution order.
pub struct FinanceMoneyProgressAdapter {
    db: Database,
}
impl FinanceMoneyProgressAdapter {
    /// 绑定提供方数据库，不执行任何读取。
    ///
    /// # 参数
    /// * `db` - 财务事实所在数据库。
    ///
    /// # 返回
    /// 返回未发起读取的适配器。
    ///
    /// # 错误
    /// 不返回错误。
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
