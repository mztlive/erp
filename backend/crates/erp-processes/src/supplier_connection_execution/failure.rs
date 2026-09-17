//! 在原结果事务中登记连接失败的 W29 事实、工作项和审计。
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::IntegrationErrorTaskId;
use erp_identity::repository::OrganizationRepository;
use erp_integration::entity::integration_ops::{
    IntegrationErrorTask, IntegrationErrorTaskData, error_owner_role,
};
use erp_integration::repository::IntegrationOpsExt;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::{SupplierApiConnection, SupplierHealthCheckRun};
use erp_supply::ports::supplier_api_gateway::ClassifiedError;
use erp_supply::service::supplier_api::context::digest;
use erp_support::BackgroundJob;
use erp_workflow::WorkItemExt;

use crate::Result;
use crate::adapters::supplier_failure::integration_class;
use crate::integration_resolution::producer::error_work_item;

pub(super) fn settle_health_failure(
    job: &mut BackgroundJob,
    run: &mut SupplierHealthCheckRun,
    at: Instant,
    latency_ms: u64,
    error: &ClassifiedError,
) -> Result<()> {
    job.record_progress(0, 0, 1, at)?;
    job.mark_failed(Some(format!("{}: {}", error.code, error.summary)), at)?;
    if error.class == SupplierFailureClass::ResultUnknown {
        run.mark_unknown(at, latency_ms, error.code.clone(), error.summary.clone())?;
    } else {
        run.fail(at, latency_ms, error.code.clone(), error.summary.clone())?;
    }
    Ok(())
}

async fn handler_org(
    db: &mongodb::Database,
    user_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<String> {
    OrganizationRepository::new(db)
        .state(executor)
        .await?
        .own_org(user_id, Instant::now())?
        .filter(|org| !org.eq_ignore_ascii_case("company"))
        .map(str::to_string)
        .ok_or_else(|| crate::Error::ValidationError("处理人缺少有效内部组织".into()))
}

pub(super) async fn persist_health_failure_task(
    db: &mongodb::Database,
    connection: &SupplierApiConnection,
    job: &BackgroundJob,
    error: &ClassifiedError,
    actor: &AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let task = IntegrationErrorTask::new(
        IntegrationErrorTaskId::new(format!("w20-error-{}", digest(&[&job.base.id]))),
        IntegrationErrorTaskData {
            message_id: None,
            business_object_id: Some(connection.base.id.clone()),
            error_class: integration_class(error.class),
            owner_role: Some(error_owner_role(integration_class(error.class)).to_string()),
            owner_user_id: Some(actor.id().to_string()),
            owner_org_unit_id: handler_org(db, actor.id(), executor).await?,
        },
    )?;
    let work_item = error_work_item(&task)?;
    let work_item_audit = actor.clone().resource_log_with_id(
        format!("w20-work-audit-{}", digest(&[&job.base.id])),
        "integration_error_task.work_item.create",
        "work_item",
        work_item.base.id.clone(),
        Some(format!("job_id={}", job.base.id)),
    )?;
    persist_failure(&MongoFailureWrite(db), &task, &work_item, &work_item_audit, executor).await
}

/// 三个真实写入步骤共用调用方结果事务；构造与 ID/时钟仍在原调用点。
trait FailureWritePort: Sync {
    type Task: Sync;
    type WorkItem: Sync;
    type Audit: Sync;
    fn task(
        &self,
        task: &Self::Task,
        executor: &mut dyn persistence_core::Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn work_item(
        &self,
        item: &Self::WorkItem,
        executor: &mut dyn persistence_core::Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn audit(
        &self,
        audit: &Self::Audit,
        executor: &mut dyn persistence_core::Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

async fn persist_failure<P: FailureWritePort>(
    port: &P,
    task: &P::Task,
    item: &P::WorkItem,
    audit: &P::Audit,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    port.task(task, executor).await?;
    port.work_item(item, executor).await?;
    port.audit(audit, executor).await
}

/// 生产写适配器只执行原三个仓储调用，不另开事务。
struct MongoFailureWrite<'a>(&'a mongodb::Database);
impl FailureWritePort for MongoFailureWrite<'_> {
    type Task = IntegrationErrorTask;
    type WorkItem = erp_workflow::WorkItem;
    type Audit = erp_audit::AuditLog;
    async fn task(&self, task: &Self::Task, executor: &mut dyn persistence_core::Executor) -> Result<()> {
        self.0.integration_error_tasks().create(task, executor).await?;
        Ok(())
    }
    async fn work_item(
        &self,
        item: &Self::WorkItem,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        self.0.work_items().create(item, executor).await?;
        Ok(())
    }
    async fn audit(&self, audit: &Self::Audit, executor: &mut dyn persistence_core::Executor) -> Result<()> {
        self.0.audit_logs().create(audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use persistence_core::Executor;

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

    struct RecordingWrite {
        identity: usize,
        fail_at: Option<usize>,
        calls: Mutex<Vec<&'static str>>,
    }
    impl RecordingWrite {
        fn record(
            &self,
            call: &'static str,
            value: u8,
            expected: u8,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!(value, expected);
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.identity);
            assert!(executor.session().is_none());
            let mut calls = self.calls.lock().unwrap();
            let position = calls.len();
            calls.push(call);
            if self.fail_at == Some(position) {
                return Err(Error::ConflictError(format!("failed {call}")));
            }
            Ok(())
        }
    }
    impl FailureWritePort for RecordingWrite {
        type Task = u8;
        type WorkItem = u8;
        type Audit = u8;
        async fn task(&self, task: &u8, executor: &mut dyn Executor) -> Result<()> {
            self.record("error_task", *task, 1, executor)
        }
        async fn work_item(&self, item: &u8, executor: &mut dyn Executor) -> Result<()> {
            self.record("work_item", *item, 2, executor)
        }
        async fn audit(&self, audit: &u8, executor: &mut dyn Executor) -> Result<()> {
            self.record("audit", *audit, 3, executor)
        }
    }

    #[tokio::test]
    async fn w29_failure_writes_share_executor_in_task_work_item_audit_order() {
        let mut executor = TestExecutor { visits: 0 };
        let port = RecordingWrite {
            identity: &mut executor as *mut TestExecutor as usize,
            fail_at: None,
            calls: Mutex::new(Vec::new()),
        };
        persist_failure(&port, &1, &2, &3, &mut executor).await.unwrap();
        assert_eq!(*port.calls.lock().unwrap(), ["error_task", "work_item", "audit"]);
        assert_eq!(executor.visits, 3);
    }

    #[tokio::test]
    async fn w29_failure_stops_at_each_write_preserving_first_error() {
        let expected = ["error_task", "work_item", "audit"];
        for fail_at in 0..expected.len() {
            let mut executor = TestExecutor { visits: 0 };
            let port = RecordingWrite {
                identity: &mut executor as *mut TestExecutor as usize,
                fail_at: Some(fail_at),
                calls: Mutex::new(Vec::new()),
            };
            let error = persist_failure(&port, &1, &2, &3, &mut executor).await.unwrap_err();
            assert!(
                matches!(error, Error::ConflictError(message) if message == format!("failed {}", expected[fail_at]))
            );
            assert_eq!(*port.calls.lock().unwrap(), expected[..=fail_at]);
            assert_eq!(executor.visits, fail_at + 1);
        }
    }
}
