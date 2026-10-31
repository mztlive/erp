//! 将供应链窄资金 Port 装配为财务真实占用，归属全部从服务端读取。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_finance::entity::supplier_deposit::SupplierDepositAccount;
use erp_finance::repository::SupplierDepositExt;
use erp_finance::service::supplier_deposit::SupplierDepositService;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt};
use erp_supply::ports::connector::funds::{OrderFundsCoverage, OrderFundsQuery, SupplierOrderFunds};
use erp_supply::repository::SupplierApiExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 资金资格 adapter 不接受前端指定账户或付款主体。
pub struct SupplierDepositFundsAdapter {
    db: Database,
}

impl SupplierDepositFundsAdapter {
    /// 绑定应用数据库。
    /// # 参数
    /// * `db` - 当前应用数据库。
    /// # 返回
    /// 返回供应链资金 Port 实现。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    async fn coverage(
        &self,
        query: &OrderFundsQuery,
        executor: &mut dyn Executor,
    ) -> Result<OrderFundsCoverage> {
        let execution = self
            .db
            .supplier_api_order_executions()
            .find_by_id(&query.execution_group_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购接口执行记录不存在".into()))?;
        if execution.connection_id != query.connection_id
            || execution.supplier_id != query.supplier_id
            || execution.purchase_order_id != query.purchase_order_id
            || execution.purchase_order_revision_id != query.purchase_revision_id
        {
            return Err(Error::BusinessLogicError("资金资格来源与冻结采购执行不一致".into()));
        }
        let account = self.bound_account(&execution, executor).await?;
        let reservation = SupplierDepositService::new(self.db.clone())
            .execution_coverage(
                &execution.base.id,
                &execution.purchase_order_id,
                &execution.purchase_order_revision_id,
                query.approved_total,
                Instant::now(),
                executor,
            )
            .await?;
        if reservation.account_id != account.base.id {
            return Err(Error::BusinessLogicError("原资金占用不属于当前连接账号".into()));
        }
        Ok(OrderFundsCoverage {
            reservation_id: reservation.base.id,
            account_id: account.base.id,
            approved_total: query.approved_total,
        })
    }
    async fn bound_account(
        &self,
        execution: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositAccount> {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&execution.connection_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".into()))?;
        let account_id = connection
            .supplier_deposit_account_id
            .as_deref()
            .ok_or_else(|| Error::BusinessLogicError("请先核验并绑定供应商预存款账户".into()))?;
        let account = self
            .db
            .supplier_deposit_accounts()
            .find_by_id(account_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("连接预存款账户不存在".into()))?;
        let revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&execution.purchase_order_revision_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("冻结采购版本不存在".into()))?;
        if connection.supplier_id.as_ref() != execution.supplier_id
            || account.identity.supplier_id != execution.supplier_id
            || account.identity.payer_party_id != revision.payment_subject.settlement_party_id.as_ref()
            || connection.supplier_deposit_external_account_ref.as_ref()
                != Some(&account.identity.external_account_ref)
        {
            return Err(Error::BusinessLogicError("连接、付款主体或外部资金账号与资金保障不一致".into()));
        }
        Ok(account)
    }
}

#[async_trait]
impl SupplierOrderFunds for SupplierDepositFundsAdapter {
    type Error = Error;
    async fn execution_coverage(
        &self,
        query: &OrderFundsQuery,
        executor: &mut dyn Executor,
    ) -> Result<OrderFundsCoverage> {
        self.coverage(query, executor).await
    }
    async fn mark_sent(&self, query: &OrderFundsQuery, executor: &mut dyn Executor) -> Result<()> {
        let coverage = self.coverage(query, executor).await?;
        SupplierDepositService::new(self.db.clone())
            .mark_external_write_sent(&coverage.reservation_id, executor)
            .await?;
        Ok(())
    }
}
