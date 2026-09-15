//! 集成事实、正式 WorkItem、审计的原顺序写入；复用外层唯一 Executor。
use async_trait::async_trait;
use erp_audit::{AuditExt, AuditLog};
use erp_integration::entity::integration_ops::{
    InboxMessage, IntegrationErrorTask, ReconciliationDifference,
};
use erp_integration::service::error_task::persist_error_task;
use erp_integration::service::inbox_message::persist_error_task_with_message_failure;
use erp_integration::service::reconciliation_difference::persist_difference;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

pub(super) enum CreatedFact<'a> {
    ErrorTask(&'a IntegrationErrorTask),
    Difference(&'a ReconciliationDifference),
    FailedMessage { task: &'a IntegrationErrorTask, message: &'a mut InboxMessage },
}
#[async_trait]
trait CreationWrites: Send {
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn work_item(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
}
async fn execute(writes: &mut impl CreationWrites, executor: &mut dyn Executor) -> Result<()> {
    writes.domain(executor).await?;
    writes.work_item(executor).await?;
    writes.audit(executor).await
}
struct MongoWrites<'a> {
    db: &'a Database,
    fact: CreatedFact<'a>,
    work_item: &'a WorkItem,
    audit: &'a AuditLog,
}
#[async_trait]
impl CreationWrites for MongoWrites<'_> {
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<()> {
        match &mut self.fact {
            CreatedFact::ErrorTask(task) => persist_error_task(self.db, task, executor).await?,
            CreatedFact::Difference(difference) => persist_difference(self.db, difference, executor).await?,
            CreatedFact::FailedMessage { task, message } => {
                persist_error_task_with_message_failure(self.db, task, message, executor).await?
            },
        }
        Ok(())
    }
    async fn work_item(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.db.work_items().create(self.work_item, executor).await?;
        Ok(())
    }
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.db.audit_logs().create(self.audit, executor).await?;
        Ok(())
    }
}
/// 按本域事实、正式任务、审计的顺序执行原事务内写入。
/// # Errors
/// 任一步失败保留原错误且停止后续写入。
pub(super) async fn persist_created(
    db: &Database,
    fact: CreatedFact<'_>,
    work_item: &WorkItem,
    audit: &AuditLog,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(&mut MongoWrites { db, fact, work_item, audit }, executor).await
}
#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use persistence_core::Executor;

    use super::{CreationWrites, execute};
    use crate::{Error, Result};
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    #[derive(Default)]
    struct RecordingWrites {
        steps: Vec<&'static str>,
        executors: Vec<usize>,
        fail: Option<&'static str>,
    }
    impl RecordingWrites {
        fn record(&mut self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.steps.push(step);
            self.executors.push(executor as *mut dyn Executor as *mut () as usize);
            if self.fail == Some(step) {
                return Err(Error::ConflictError(step.into()));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl CreationWrites for RecordingWrites {
        async fn domain(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("domain", e)
        }
        async fn work_item(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("work_item", e)
        }
        async fn audit(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("audit", e)
        }
    }
    #[tokio::test]
    async fn integration_creation_preserves_one_executor_and_write_order() {
        let mut e = TestExecutor { _identity: 1 };
        let id = &mut e as *mut TestExecutor as usize;
        let mut writes = RecordingWrites::default();
        execute(&mut writes, &mut e).await.unwrap();
        assert_eq!(writes.steps, ["domain", "work_item", "audit"]);
        assert_eq!(writes.executors, [id, id, id]);
    }
    #[tokio::test]
    async fn integration_creation_stops_at_each_original_failure() {
        let expected = ["domain", "work_item", "audit"];
        for (at, fail) in expected.iter().enumerate() {
            let mut e = TestExecutor { _identity: 1 };
            let mut writes = RecordingWrites { fail: Some(*fail), ..Default::default() };
            let result = execute(&mut writes, &mut e).await;
            assert!(matches!(result,Err(Error::ConflictError(message)) if message==*fail));
            assert_eq!(writes.steps, expected[..=at]);
        }
    }
}
