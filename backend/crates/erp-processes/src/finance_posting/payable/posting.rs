//! 付款过账跨域步骤；统一传递调用方 Executor 并在首个失败处停止。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_finance::entity::payable::{PendingPaymentAllocation, SupplierPayment};
use erp_finance::service::payable::{
    PaymentSettlement, finish_supplier_payment_in_transaction, settle_supplier_payment_in_transaction,
};
use mongodb::Database;
use persistence_core::Executor;

use super::payment_task;
use crate::{Error, Result};
/// 付款过账授权来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PaymentPostSource {
    /// 当前开放付款执行任务。
    ExecutionTask,
}

/// 在调用方事务内写入付款核销、应付余额、任务进度与审计。
///
/// 数据面职责归位（FIN-E02/FIN-R05）：分录/子账事实一次批量装载并去重，
/// 核销净额、逐分录开放余额、连续序号与分配实体构造由
/// [`erp_finance::entity::payable::PaymentAllocationLedger`] 完成；子账进度按账户聚合后批量条件更新，
/// 分配行批量插入。供应商一致性、事务、任务同步与审计仍在本方法编排。
///
/// # 错误
/// 付款状态、供应商、应付开放余额、分配金额或仓储写入不合法时返回错误。
pub(super) async fn post_supplier_payment_in_transaction(
    db: &Database,
    payment: &mut SupplierPayment,
    pending: &[PendingPaymentAllocation],
    source: PaymentPostSource,
    actor: &AuditActor,
    session: &mut dyn Executor,
) -> Result<()> {
    let mut steps = MongoPaymentPosting { db, payment, pending, source, actor, settlement: None };
    execute_posting(&mut steps, session).await
}

/// 根事务的付款步骤合同；实现只执行本步骤，不创建新事务。
#[async_trait]
trait PaymentPostingSteps: Send {
    async fn settle_accounts(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn synchronize_tasks(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn persist_payment(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn write_audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
}

/// 保持原余额更新、任务同步、付款/分配、审计顺序；所有步骤收到相同 Executor。
async fn execute_posting(steps: &mut impl PaymentPostingSteps, executor: &mut dyn Executor) -> Result<()> {
    steps.settle_accounts(executor).await?;
    steps.synchronize_tasks(executor).await?;
    steps.persist_payment(executor).await?;
    steps.write_audit(executor).await
}

struct MongoPaymentPosting<'a> {
    db: &'a Database,
    payment: &'a mut SupplierPayment,
    pending: &'a [PendingPaymentAllocation],
    source: PaymentPostSource,
    actor: &'a AuditActor,
    settlement: Option<PaymentSettlement>,
}

impl MongoPaymentPosting<'_> {
    fn settlement(&self) -> Result<&PaymentSettlement> {
        self.settlement.as_ref().ok_or_else(|| Error::Internal("付款过账缺少应付核销结果".to_string()))
    }
}

#[async_trait]
impl PaymentPostingSteps for MongoPaymentPosting<'_> {
    async fn settle_accounts(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.settlement = Some(
            settle_supplier_payment_in_transaction(
                self.db,
                self.payment,
                self.pending,
                self.actor.id(),
                executor,
            )
            .await?,
        );
        Ok(())
    }

    async fn synchronize_tasks(&mut self, executor: &mut dyn Executor) -> Result<()> {
        for account_id in &self.settlement()?.applied_account_ids {
            payment_task::sync_purchase_payment_task(self.db, account_id, executor).await?;
        }
        Ok(())
    }

    async fn persist_payment(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let settlement = self
            .settlement
            .as_ref()
            .ok_or_else(|| Error::Internal("付款过账缺少应付核销结果".to_string()))?;
        match self.source {
            PaymentPostSource::ExecutionTask => {
                finish_supplier_payment_in_transaction(
                    self.db,
                    self.payment,
                    self.pending,
                    settlement,
                    executor,
                )
                .await?
            },
        }
        Ok(())
    }

    async fn write_audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let audit = self.actor.clone().resource_log(
            "supplier_payment.post",
            "supplier_payment",
            self.payment.base.id.clone(),
        )?;
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 非零大小保证执行器地址能够区分不同实例。
    struct TestExecutor {
        _identity: u8,
    }

    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

    struct RecordedPosting {
        calls: Vec<(&'static str, usize)>,
        fail_at: Option<&'static str>,
    }

    impl RecordedPosting {
        fn record(&mut self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.calls.push((step, executor as *mut dyn Executor as *mut () as usize));
            if self.fail_at == Some(step) {
                return Err(Error::ConflictError(step.to_string()));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl PaymentPostingSteps for RecordedPosting {
        async fn settle_accounts(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("settle", e)
        }
        async fn synchronize_tasks(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("tasks", e)
        }
        async fn persist_payment(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("payment", e)
        }
        async fn write_audit(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("audit", e)
        }
    }

    #[tokio::test]
    async fn posting_keeps_original_order_and_the_same_executor() {
        let mut executor = TestExecutor { _identity: 1 };
        let expected_executor = &mut executor as *mut TestExecutor as usize;
        let mut steps = RecordedPosting { calls: Vec::new(), fail_at: None };
        execute_posting(&mut steps, &mut executor).await.unwrap();
        assert_eq!(
            steps.calls,
            vec![
                ("settle", expected_executor),
                ("tasks", expected_executor),
                ("payment", expected_executor),
                ("audit", expected_executor)
            ]
        );
    }

    #[tokio::test]
    async fn posting_stops_at_each_failure_and_preserves_the_original_error() {
        let sequence = ["settle", "tasks", "payment", "audit"];
        for (index, fail_at) in sequence.iter().enumerate() {
            let mut steps = RecordedPosting { calls: Vec::new(), fail_at: Some(fail_at) };
            let error = execute_posting(&mut steps, &mut TestExecutor { _identity: 1 }).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == *fail_at));
            assert_eq!(steps.calls.iter().map(|(step, _)| *step).collect::<Vec<_>>(), sequence[..=index]);
        }
    }
}
