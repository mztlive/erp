//! 付款冲正、应付冲减、付款工作项与审计的同事务逆向流程。

use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::ids::PayableAccountId;
use erp_finance::entity::payable::SupplierPayment;
use erp_finance::ports::payment_restoration::PaymentRequestRestorationPort;
use erp_finance::service::payable::payment_reversal::{
    PaymentReversalFact, PaymentReversalWrite, load_posted_payment_for_reversal, prepare_payment_reversal,
};
use erp_returns::entity::returns::PaymentReversal;
use erp_returns::service::ReturnsService;
use mongodb::Database;
use persistence_core::Executor;

use super::adapter::execute_payment_reversal_domain_action;
use crate::{Error, Result};

/// 付款冲正最终通过流程；所有财务与工作项副作用复用调用方事务。
pub struct PaymentReversalProcess {
    db: Database,
    restoration: Arc<dyn PaymentRequestRestorationPort<Error = Error>>,
}
impl PaymentReversalProcess {
    /// 绑定组合根数据库；审批运行时已完成授权。
    ///
    /// # 参数
    /// * `db` - 组合根数据库。
    /// * `restoration` - 应用装配的原付款申请恢复端口。
    ///
    /// # 返回
    /// 返回付款冲正最终过账流程。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, restoration: Arc<dyn PaymentRequestRestorationPort<Error = Error>>) -> Self {
        Self { db, restoration }
    }

    /// 在审批最终通过持有的唯一事务内执行付款冲正。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID。
    /// * `actor` - 已通过鉴权的审计操作人。
    /// * `executor` - 审批最终通过持有的执行器；本函数不再开启事务。
    ///
    /// # 返回
    /// 成功时无返回值。冲正过账已写入该执行器。
    ///
    /// # 错误
    /// 非审批中、原付款不存在、累计冲正超额或仓储写入失败时返回错误。
    pub async fn post_payment_reversal_apply(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        apply_payment_reversal_final_post(
            &self.db,
            self.restoration.as_ref(),
            id,
            actor.id(),
            actor,
            executor,
        )
        .await
    }
}

/// 在最终通过事务内执行过账副作用并写回冲正单。
///
/// # 错误
/// 非审批中、原付款不存在或仓储失败时返回错误。
async fn apply_payment_reversal_final_post(
    db: &Database,
    restoration: &dyn PaymentRequestRestorationPort<Error = Error>,
    reversal_id: &str,
    actor_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute_final_post(&MongoPaymentReversal { db, restoration, reversal_id, actor_id, actor }, executor)
        .await
}

/// 生产与替身共用的最终通过步骤；账户集合保持 HashSet 原迭代顺序。
#[async_trait::async_trait]
trait PaymentReversalPostingPort: Send + Sync {
    type Reversal: Send + Sync;
    type Payment: Send + Sync;
    type Write: Send;
    async fn prepare(&self, executor: &mut dyn Executor) -> Result<Self::Reversal>;
    async fn payment(&self, reversal: &Self::Reversal, executor: &mut dyn Executor) -> Result<Self::Payment>;
    async fn limit(
        &self,
        reversal: &Self::Reversal,
        payment: &Self::Payment,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn settlements(
        &self,
        reversal: &Self::Reversal,
        payment: Self::Payment,
        executor: &mut dyn Executor,
    ) -> Result<(Self::Write, HashSet<PayableAccountId>)>;
    async fn task(&self, account_id: &PayableAccountId, executor: &mut dyn Executor) -> Result<()>;
    async fn reverse_finance(&self, write: Self::Write, executor: &mut dyn Executor) -> Result<()>;
    async fn post_reversal(&self, reversal: &mut Self::Reversal, executor: &mut dyn Executor) -> Result<()>;
    async fn audit(&self, reversal: &Self::Reversal, executor: &mut dyn Executor) -> Result<()>;
}

async fn execute_final_post<P: PaymentReversalPostingPort>(
    port: &P,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut reversal = port.prepare(executor).await?;
    let payment = port.payment(&reversal, executor).await?;
    port.limit(&reversal, &payment, executor).await?;

    let (write, affected_accounts) = port.settlements(&reversal, payment, executor).await?;
    for account_id in affected_accounts {
        port.task(&account_id, executor).await?;
    }

    port.reverse_finance(write, executor).await?;
    port.post_reversal(&mut reversal, executor).await?;
    port.audit(&reversal, executor).await
}

struct MongoPaymentReversal<'a> {
    db: &'a Database,
    restoration: &'a dyn PaymentRequestRestorationPort<Error = Error>,
    reversal_id: &'a str,
    actor_id: &'a str,
    actor: &'a AuditActor,
}

#[async_trait::async_trait]
impl PaymentReversalPostingPort for MongoPaymentReversal<'_> {
    type Reversal = PaymentReversal;
    type Payment = SupplierPayment;
    type Write = PaymentReversalWrite;

    async fn prepare(&self, executor: &mut dyn Executor) -> Result<Self::Reversal> {
        let mut reversal = ReturnsService::new(self.db.clone())
            .prepare_payment_reversal_post(self.reversal_id, executor)
            .await?;
        execute_payment_reversal_domain_action(
            &mut reversal,
            erp_workflow::service::approval::policy::ApprovalDomainAction::PaymentReversalPost,
        )?;
        Ok(reversal)
    }

    async fn payment(&self, reversal: &Self::Reversal, executor: &mut dyn Executor) -> Result<Self::Payment> {
        Ok(load_posted_payment_for_reversal(self.db, &reversal.original_supplier_payment_id, executor)
            .await?)
    }

    async fn limit(
        &self,
        reversal: &Self::Reversal,
        payment: &Self::Payment,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ReturnsService::new(self.db.clone())
            .validate_payment_reversal_amount(reversal, payment.amount, executor)
            .await?;
        Ok(())
    }

    async fn settlements(
        &self,
        reversal: &Self::Reversal,
        payment: Self::Payment,
        executor: &mut dyn Executor,
    ) -> Result<(Self::Write, HashSet<PayableAccountId>)> {
        Ok(prepare_payment_reversal(
            self.db,
            payment,
            PaymentReversalFact { amount: reversal.amount, occurred_at: reversal.occurred_at },
            self.actor_id,
            executor,
        )
        .await?)
    }

    async fn task(&self, _account_id: &PayableAccountId, _executor: &mut dyn Executor) -> Result<()> {
        // 旧采购应付直付任务已停止；原 sync_purchase_payment_task 在此同样不执行写入。
        Ok(())
    }

    async fn reverse_finance(&self, write: Self::Write, executor: &mut dyn Executor) -> Result<()> {
        let (payment, allocations) = write.payment_request_reversal(self.db, executor).await?;
        self.restoration.restore(&payment, &allocations, self.actor, executor).await?;
        write.persist(self.db, executor).await?;
        Ok(())
    }

    async fn post_reversal(&self, reversal: &mut Self::Reversal, executor: &mut dyn Executor) -> Result<()> {
        ReturnsService::new(self.db.clone()).persist_payment_reversal_post(reversal, executor).await?;
        Ok(())
    }

    async fn audit(&self, reversal: &Self::Reversal, executor: &mut dyn Executor) -> Result<()> {
        let audit = self.actor.clone().resource_log(
            "payment_reversal.post",
            "payment_reversal",
            reversal.base.id.clone(),
        )?;
        persist_log(self.db, &audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use persistence_core::ReadExecutor as TestExecutor;

    use super::*;
    use crate::Error;
    struct RecordingPort {
        identity: usize,
        fail_at: Option<usize>,
        calls: Mutex<Vec<&'static str>>,
    }
    impl RecordingPort {
        fn record(&self, name: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.identity);
            executor.session();
            let mut calls = self.calls.lock().unwrap();
            let index = calls.len();
            calls.push(name);
            if self.fail_at == Some(index) {
                return Err(Error::ConflictError(format!("failure {index}")));
            }
            Ok(())
        }
    }
    #[async_trait::async_trait]
    impl PaymentReversalPostingPort for RecordingPort {
        type Reversal = ();
        type Payment = ();
        type Write = ();
        async fn prepare(&self, e: &mut dyn Executor) -> Result<()> {
            self.record("reversal_guard", e)
        }
        async fn payment(&self, _: &(), e: &mut dyn Executor) -> Result<()> {
            self.record("posted_payment", e)
        }
        async fn limit(&self, _: &(), _: &(), e: &mut dyn Executor) -> Result<()> {
            self.record("limit", e)
        }
        async fn settlements(
            &self,
            _: &(),
            _: (),
            e: &mut dyn Executor,
        ) -> Result<((), HashSet<PayableAccountId>)> {
            self.record("settlements", e)?;
            Ok(((), [PayableAccountId::new("a1"), PayableAccountId::new("a2")].into_iter().collect()))
        }
        async fn task(&self, _: &PayableAccountId, e: &mut dyn Executor) -> Result<()> {
            self.record("payment_task", e)
        }
        async fn reverse_finance(&self, _: (), e: &mut dyn Executor) -> Result<()> {
            self.record("reverse_allocations_then_payment", e)
        }
        async fn post_reversal(&self, _: &mut (), e: &mut dyn Executor) -> Result<()> {
            self.record("post_reversal", e)
        }
        async fn audit(&self, _: &(), e: &mut dyn Executor) -> Result<()> {
            self.record("audit", e)
        }
    }
    const STEPS: &[&str] = &[
        "reversal_guard",
        "posted_payment",
        "limit",
        "settlements",
        "payment_task",
        "payment_task",
        "reverse_allocations_then_payment",
        "post_reversal",
        "audit",
    ];

    #[tokio::test]
    async fn payment_tasks_stay_between_all_settlements_and_reverse_allocations() {
        let mut executor = TestExecutor::default();
        let port = RecordingPort {
            identity: (&mut executor as *mut TestExecutor) as usize,
            fail_at: None,
            calls: Mutex::new(Vec::new()),
        };

        execute_final_post(&port, &mut executor).await.unwrap();
        assert_eq!(*port.calls.lock().unwrap(), STEPS);
    }

    #[tokio::test]
    async fn payment_reversal_stops_at_each_failure_without_later_writes() {
        for index in 0..STEPS.len() {
            let mut executor = TestExecutor::default();
            let port = RecordingPort {
                identity: (&mut executor as *mut TestExecutor) as usize,
                fail_at: Some(index),
                calls: Mutex::new(Vec::new()),
            };

            let error = execute_final_post(&port, &mut executor).await.unwrap_err();
            assert!(matches!(error,Error::ConflictError(message) if message==format!("failure {index}")));
            assert_eq!(*port.calls.lock().unwrap(), STEPS[..=index]);
        }
    }
}
