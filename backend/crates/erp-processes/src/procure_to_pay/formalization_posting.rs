//! 采购正式化生产步骤与调用方唯一执行器的顺序合同。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditLog;
use mongodb::Database;
use persistence_core::Executor;

use super::review::{FormalizedOrderPersist, FormalizedPurchaseEffects};
use crate::audit::persist_log;
use crate::{Error, Result};

/// 采购正式化与跨域后续写入的既有边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Purchase,
    Payable,
    PaymentTask,
    Costs,
    Fulfillment,
    Audit,
}
#[async_trait]
trait PostingSteps: Send {
    /// 在同一执行器中完成当前步骤，失败立即传播给根事务。
    async fn apply(&mut self, step: Step, executor: &mut dyn Executor) -> Result<()>;
}
/// 生产流程与失败替身共同使用的顺序入口。
async fn execute(steps: &mut impl PostingSteps, executor: &mut dyn Executor) -> Result<()> {
    for step in
        [Step::Purchase, Step::Payable, Step::PaymentTask, Step::Costs, Step::Fulfillment, Step::Audit]
    {
        steps.apply(step, executor).await?;
    }
    Ok(())
}
struct MongoPosting<'a> {
    db: &'a Database,
    actor: &'a AuditActor,
    audit: &'a AuditLog,
    persist: Option<FormalizedOrderPersist>,
    effects: Option<FormalizedPurchaseEffects>,
}
impl MongoPosting<'_> {
    /// 读取采购步骤留下的应付与履约事实；计划尚未产生或已被消费时返回内部错误。
    fn effects(&self) -> Result<&FormalizedPurchaseEffects> {
        self.effects.as_ref().ok_or_else(|| Error::Internal("采购正式化写入计划缺少采购结果".to_string()))
    }
}
#[async_trait]
impl PostingSteps for MongoPosting<'_> {
    /// 按固定步骤消费写入计划；计划缺失或该步失败时保留原错误。
    ///
    /// # 参数
    /// * `step` - 采购、应付、付款任务、成本、履约或审计。
    /// * `executor` - 调用方唯一执行器。
    ///
    /// # 返回
    /// 该步写入完成。采购步消费写入计划，并把采购结果留给后续步骤。
    ///
    /// # 错误
    /// 采购写入计划已消费，或后续步骤缺少采购结果时返回 `Internal`。领域写入或审计失败时返回原错误。
    async fn apply(&mut self, step: Step, executor: &mut dyn Executor) -> Result<()> {
        match step {
            Step::Purchase => {
                let persist = self
                    .persist
                    .take()
                    .ok_or_else(|| Error::Internal("采购正式化写入计划已消费".to_string()))?;
                self.effects = Some(persist.persist_order(self.db, self.actor, executor).await?);
            },
            Step::Payable => {
                let payable = self.effects()?.payable();
                erp_finance::service::payable::purchase_initial::persist(
                    self.db, &payable.0, &payable.1, executor,
                )
                .await?;
            },
            Step::PaymentTask => {
                let payable = self.effects()?.payable();
                crate::finance_posting::payable::payment_task::ensure_purchase_payment_task(
                    self.db, &payable.0, &payable.1, executor,
                )
                .await?;
            },
            Step::Costs => {
                erp_finance::service::cost::purchase_initial::persist(
                    self.db,
                    self.effects()?.cost_entries(),
                    executor,
                )
                .await?
            },
            Step::Fulfillment => {
                let effects = self
                    .effects
                    .take()
                    .ok_or_else(|| Error::Internal("采购正式化写入计划缺少采购结果".to_string()))?;
                effects.persist_fulfillment(self.db, self.actor.id(), executor).await?;
            },
            Step::Audit => {
                persist_log(self.db, self.audit, executor).await?;
            },
        }
        Ok(())
    }
}
/// 在根事务原位置执行采购、应付、付款任务、成本、履约与审计；不新建事务或重排步骤。
///
/// # 参数
/// * `db` - 各步写入使用的数据库。
/// * `persist` - 事务外算好的采购形式化写入计划。
/// * `actor` - 生效操作人。
/// * `audit` - 成功路径上的审计日志。
/// * `executor` - 调用方唯一执行器。
///
/// # 返回
/// 六步按既定顺序写完。
///
/// # 错误
/// 任一步失败时返回原错误，不再执行后续步骤。
pub(super) async fn post(
    db: &Database,
    persist: FormalizedOrderPersist,
    actor: &AuditActor,
    audit: &AuditLog,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(&mut MongoPosting { db, actor, audit, persist: Some(persist), effects: None }, executor).await
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct RecordingSteps {
        calls: Vec<Step>,
        executor: usize,
        fail_at: Option<Step>,
    }
    #[async_trait]
    impl PostingSteps for RecordingSteps {
        async fn apply(&mut self, step: Step, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(step);
            if Some(step) == self.fail_at {
                return Err(Error::ConflictError("原采购过账冲突".into()));
            }
            Ok(())
        }
    }
    #[tokio::test]
    async fn formalization_preserves_domain_finance_task_fulfillment_audit_order() {
        let mut executor = TestExecutor { _identity: 1 };
        let mut steps = RecordingSteps {
            calls: Vec::new(),
            executor: &mut executor as *mut TestExecutor as usize,
            fail_at: None,
        };
        execute(&mut steps, &mut executor).await.unwrap();
        assert_eq!(
            steps.calls,
            [Step::Purchase, Step::Payable, Step::PaymentTask, Step::Costs, Step::Fulfillment, Step::Audit]
        );
    }
    #[tokio::test]
    async fn every_failure_preserves_original_error_and_stops_later_writes() {
        let order =
            [Step::Purchase, Step::Payable, Step::PaymentTask, Step::Costs, Step::Fulfillment, Step::Audit];
        for (index, step) in order.into_iter().enumerate() {
            let mut executor = TestExecutor { _identity: 1 };
            let mut steps = RecordingSteps {
                calls: Vec::new(),
                executor: &mut executor as *mut TestExecutor as usize,
                fail_at: Some(step),
            };
            let error = execute(&mut steps, &mut executor).await.unwrap_err();
            assert!(matches!(error,Error::ConflictError(message) if message=="原采购过账冲突"));
            assert_eq!(steps.calls, &order[..=index]);
        }
    }
}
