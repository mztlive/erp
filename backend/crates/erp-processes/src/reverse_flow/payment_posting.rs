//! 付款冲正、应付冲减、付款工作项与审计的同事务逆向流程。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::PayableAccountId;
use erp_finance::entity::payable::SupplierPayment;
use erp_finance::service::payable::payment_reversal::{
    PaymentReversalFact, PaymentReversalWrite, load_posted_payment_for_reversal, prepare_payment_reversal,
};
use erp_identity::SharedRbacService;
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::PaymentReversalView;
use erp_returns::entity::returns::PaymentReversal;
use erp_returns::service::ReturnsService;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::adapter::execute_payment_reversal_domain_action;
use crate::Result;

/// 付款冲正最终通过流程；所有财务与工作项副作用复用调用方事务。
pub struct PaymentReversalProcess {
    db: Database,
}
impl PaymentReversalProcess {
    /// 绑定组合根数据库；审批运行时已完成授权。
    pub fn new(db: Database, _rbac: SharedRbacService) -> Self {
        Self { db }
    }

    /// 最终通过过账（§8.3-3 事务不变量，应付侧镜像）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 按原付款核销分配反向写入 `REVERSE` 分配并原子冲减应付子账已核销进度；
    /// 原付款迁移为已冲正；冲正单迁移为已过账。任一校验失败整体回滚。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单或原付款不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 累计冲正超原付款、重复过账或超额冲减
    pub async fn post_payment_reversal(&self, id: &str, actor: &AuditActor) -> Result<PaymentReversalView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let reversal_id = id.to_string();
        let detail_id = reversal_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    apply_payment_reversal_final_post(&db, &reversal_id, &actor_id, &actor_owned, session)
                        .await
                })
            })
            .await?;

        ReturnsReadService::new(self.db.clone())
            .payment_reversal_detail(&detail_id)
            .await
            .map_err(crate::Error::from)
    }

    /// 在审批最终通过持有的唯一事务内执行付款冲正。
    pub async fn post_payment_reversal_in_transaction(
        &self,
        id: &str,
        actor: &AuditActor,
        session: &mut mongodb::ClientSession,
    ) -> Result<()> {
        apply_payment_reversal_final_post(&self.db, id, actor.id(), actor, session).await
    }
}

/// 在最终通过事务内执行过账副作用并写回冲正单。
///
/// # 错误
/// 非审批中、原付款不存在或仓储失败时返回错误。
async fn apply_payment_reversal_final_post(
    db: &Database,
    reversal_id: &str,
    actor_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute_final_post(&MongoPaymentReversal { db, reversal_id, actor_id, actor }, executor).await
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

    async fn task(&self, account_id: &PayableAccountId, executor: &mut dyn Executor) -> Result<()> {
        crate::finance_posting::payable::payment_task::sync_purchase_payment_task(
            self.db, account_id, executor,
        )
        .await
    }

    async fn reverse_finance(&self, write: Self::Write, executor: &mut dyn Executor) -> Result<()> {
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
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::Error;

    struct TestExecutor {
        visits: usize,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.visits += 1;
            None
        }
    }
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
        let mut executor = TestExecutor { visits: 0 };
        let port = RecordingPort {
            identity: (&mut executor as *mut TestExecutor) as usize,
            fail_at: None,
            calls: Mutex::new(Vec::new()),
        };
        execute_final_post(&port, &mut executor).await.unwrap();
        assert_eq!(*port.calls.lock().unwrap(), STEPS);
        assert_eq!(executor.visits, STEPS.len());
    }

    #[tokio::test]
    async fn payment_reversal_stops_at_each_failure_without_later_writes() {
        for index in 0..STEPS.len() {
            let mut executor = TestExecutor { visits: 0 };
            let port = RecordingPort {
                identity: (&mut executor as *mut TestExecutor) as usize,
                fail_at: Some(index),
                calls: Mutex::new(Vec::new()),
            };
            let error = execute_final_post(&port, &mut executor).await.unwrap_err();
            assert!(matches!(error,Error::ConflictError(message) if message==format!("failure {index}")));
            assert_eq!(*port.calls.lock().unwrap(), STEPS[..=index]);
            assert_eq!(executor.visits, index + 1);
        }
    }
}
