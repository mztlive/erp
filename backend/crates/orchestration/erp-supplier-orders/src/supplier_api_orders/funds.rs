//! 发单资金保障与正式实付门槛分别执行；供应商协议要求不能被采购配置降级。
use erp_core::money::Amount;
use erp_fulfillment_execution::fulfillment_execution::ensure_prepay_gate;
use erp_party::SensitiveDataCodec;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderStage, execution_budget as read_budget};
use erp_supply::entity::execution_budget::ApprovedExecutionBudget;
use erp_supply::ports::connector::funds::{OrderFundingRequirement, OrderFundsQuery, SupplierOrderFunds};
use erp_supply::service::supplier_api::SupplierApiService;
use persistence_core::Executor;

use super::SupplierApiOrderProcess;
use crate::{Error, Result};

/// 从服务器保存的加密快照读取执行组批准预算。
/// # 参数
/// `execution` 必须由调用方按当前权限读取，`codec` 为应用敏感资料加密器。
/// # 返回
/// 与原执行组及采购修订精确绑定的商品、运费和总预算。
/// # 错误
/// 快照缺失、历史记录无预算或来源版本不符时拒绝，不能采用客户端金额。
pub fn execution_budget(
    execution: &ApiOrderExecution,
    codec: &SensitiveDataCodec,
) -> Result<ApprovedExecutionBudget> {
    let encrypted = execution
        .request_encrypted
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("执行组尚未冻结批准预算".into()))?;
    Ok(read_budget(execution, &codec.decrypt(encrypted)?)?)
}

impl SupplierApiOrderProcess {
    pub(super) async fn ensure_preflight_funds(
        &self,
        execution: &ApiOrderExecution,
        purchase: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, executor)
            .await?;
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        match connector.funding_requirement() {
            OrderFundingRequirement::PostedPaymentGate => {
                ensure_prepay_gate(&self.db, executor, purchase).await.map_err(Into::into)
            },
            OrderFundingRequirement::VerifiedSupplierDeposit => Ok(()),
        }
    }

    pub(super) async fn ensure_execution_funds(
        &self,
        execution: &ApiOrderExecution,
        purchase: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, executor)
            .await?;
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        match connector.funding_requirement() {
            OrderFundingRequirement::PostedPaymentGate => {
                ensure_prepay_gate(&self.db, executor, purchase).await.map_err(Into::into)
            },
            OrderFundingRequirement::VerifiedSupplierDeposit => {
                if !matches!(
                    execution.stage,
                    ApiOrderStage::Create
                        | ApiOrderStage::ConfirmOrder
                        | ApiOrderStage::CreationUnknown
                        | ApiOrderStage::ConfirmationUnknown
                ) {
                    return Ok(());
                }
                let snapshot = self.decode(execution)?;
                require_coverage(self.funds.as_ref(), execution, snapshot.approved_total, executor).await
            },
        }
    }
    pub(super) async fn mark_funds_sent(
        &self,
        execution: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, executor)
            .await?;
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        if connector.funding_requirement() == OrderFundingRequirement::VerifiedSupplierDeposit {
            let snapshot = self.decode(execution)?;
            self.funds.mark_sent(&funds_query(execution, snapshot.approved_total), executor).await?;
        }
        Ok(())
    }
}

/// 使用原执行身份和服务端冻结预算构造资金资格查询。
fn funds_query(execution: &ApiOrderExecution, approved_total: Amount) -> OrderFundsQuery {
    OrderFundsQuery {
        execution_group_id: execution.base.id.clone(),
        connection_id: execution.connection_id.clone(),
        supplier_id: execution.supplier_id.clone(),
        purchase_order_id: execution.purchase_order_id.clone(),
        purchase_revision_id: execution.purchase_order_revision_id.clone(),
        approved_total,
    }
}

/// 使用注入的 ERP 资金资格端口核验原执行，保留存储及未知结果错误。
async fn require_coverage(
    funds: &dyn SupplierOrderFunds<Error = Error>,
    execution: &ApiOrderExecution,
    approved_total: Amount,
    executor: &mut dyn Executor,
) -> Result<()> {
    let query = funds_query(execution, approved_total);
    let coverage = funds.execution_coverage(&query, executor).await?;
    coverage.ensure_matches(&query)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use erp_supply::ports::connector::funds::OrderFundsCoverage;
    use persistence_core::NoTransaction;

    use super::*;

    struct Funds {
        queries: Mutex<Vec<OrderFundsQuery>>,
        amount: Amount,
        failure: bool,
    }

    #[async_trait]
    impl SupplierOrderFunds for Funds {
        type Error = Error;
        async fn execution_coverage(
            &self,
            query: &OrderFundsQuery,
            _: &mut dyn Executor,
        ) -> Result<OrderFundsCoverage> {
            self.queries.lock().unwrap().push(query.clone());
            if self.failure {
                return Err(Error::ConflictError("原资金占用已变化".into()));
            }
            Ok(OrderFundsCoverage {
                reservation_id: "reservation".into(),
                account_id: "account".into(),
                approved_total: self.amount,
            })
        }
        async fn mark_sent(&self, _: &OrderFundsQuery, _: &mut dyn Executor) -> Result<()> {
            panic!("资格读取不得登记已发送或形成付款")
        }
    }

    fn execution() -> ApiOrderExecution {
        ApiOrderExecution::queued(
            "execution".into(),
            "purchase".into(),
            "revision".into(),
            "connection".into(),
            "supplier".into(),
            vec![],
        )
    }

    #[tokio::test]
    async fn funds_port_receives_frozen_identity_and_preserves_failures() {
        let funds = Funds { queries: Mutex::new(vec![]), amount: "100".parse().unwrap(), failure: false };
        require_coverage(&funds, &execution(), funds.amount, &mut NoTransaction).await.unwrap();
        assert_eq!(
            funds.queries.lock().unwrap().as_slice(),
            &[OrderFundsQuery {
                execution_group_id: "execution".into(),
                connection_id: "connection".into(),
                supplier_id: "supplier".into(),
                purchase_order_id: "purchase".into(),
                purchase_revision_id: "revision".into(),
                approved_total: funds.amount,
            }]
        );
        let failure = Funds { failure: true, ..funds };
        assert!(matches!(require_coverage(&failure, &execution(), failure.amount, &mut NoTransaction).await,
            Err(Error::ConflictError(message)) if message == "原资金占用已变化"));
    }

    #[tokio::test]
    async fn a_successful_port_cannot_cover_a_different_budget() {
        let funds = Funds { queries: Mutex::new(vec![]), amount: "99".parse().unwrap(), failure: false };
        assert!(
            require_coverage(&funds, &execution(), "100".parse().unwrap(), &mut NoTransaction).await.is_err()
        );
    }
}
