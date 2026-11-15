//! 在原结果事务中登记连接失败的 W29 事实、工作项和审计。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
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
#[cfg(test)]
use persistence_core::ReadExecutor;

use crate::Result;
use crate::integration_resolution::producer::error_work_item;

/// 把分类失败写入任务进度和健康检查运行记录；未知结果与明确失败分开登记。
///
/// # 参数
/// * `job` - 待标失败的后台任务。
/// * `run` - 同一次健康检查运行。
/// * `at` - 失败登记时刻。
/// * `latency_ms` - 网关耗时毫秒。
/// * `error` - 已分类的网关失败。
///
/// # 返回
/// 内存中的任务和运行记录更新成功时返回。
///
/// # 错误
/// 进度、失败标记或运行记录状态迁移失败时返回错误。`ResultUnknown` 记为未知，其余记为失败。
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

/// 处理人必须有非 `company` 的有效主属组织，否则不能登记失败任务。
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

/// 在调用方结果事务中写入集成错误任务、工作项和对应审计。
///
/// # 参数
/// * `db` - 连接与组织所在数据库。
/// * `connection` - 失败所属连接。
/// * `job` - 已标失败的后台任务。
/// * `error` - 已分类失败。
/// * `actor` - 审计操作人，同时作为处理人。
/// * `executor` - 调用方结果事务执行器。
///
/// # 返回
/// 错误任务、工作项和审计都写入后返回。
///
/// # 错误
/// 处理人组织无效、任务或工作项构造失败，或任一写入失败时返回错误。
pub(super) async fn persist_health_failure_task(
    db: &mongodb::Database,
    connection: &SupplierApiConnection,
    job: &BackgroundJob,
    error: &ClassifiedError,
    actor: &AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let mut task = IntegrationErrorTask::new(
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
    record_initial_failure(&mut task, error)?;
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

/// 首次失败也属于执行尝试；沿用脱敏摘要字段，不改变原任务身份。
fn record_initial_failure(task: &mut IntegrationErrorTask, error: &ClassifiedError) -> Result<()> {
    let summary = format!("{}: {}", error.code, error.summary).chars().take(512).collect();
    task.record_attempt(Instant::now(), Some(summary))?;
    Ok(())
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
        persist_log(self.0, audit, executor).await?;
        Ok(())
    }
}

fn integration_class(class: SupplierFailureClass) -> erp_integration::entity::integration_ops::ErrorClass {
    use erp_integration::entity::integration_ops::ErrorClass;
    match class {
        SupplierFailureClass::CapabilityGap => ErrorClass::CapabilityGap,
        SupplierFailureClass::MappingError => ErrorClass::MappingError,
        SupplierFailureClass::BusinessRejected => ErrorClass::BusinessRejected,
        SupplierFailureClass::TransientFailure => ErrorClass::TransientFailure,
        SupplierFailureClass::ResultUnknown => ErrorClass::ResultUnknown,
        SupplierFailureClass::AuthSignature => ErrorClass::AuthSignature,
        SupplierFailureClass::RateLimited => ErrorClass::RateLimited,
        SupplierFailureClass::OutOfOrder => ErrorClass::OutOfOrder,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_integration::entity::integration_ops::ErrorClass;
    use persistence_core::Executor;

    use super::*;
    use crate::Error;

    #[test]
    fn initial_failure_records_reason_and_bounds_summary() {
        let mut task = IntegrationErrorTask::with_derived_owner_role(
            IntegrationErrorTaskId::new("error-1"),
            None,
            Some("connection-1".into()),
            ErrorClass::AuthSignature,
            "admin".into(),
            "org-1".into(),
        )
        .unwrap();
        let mut error = ClassifiedError {
            class: SupplierFailureClass::AuthSignature,
            code: "TOKEN_EXPIRED".into(),
            summary: "授权已过期".into(),
        };
        record_initial_failure(&mut task, &error).unwrap();
        assert_eq!(task.attempt_count, 1);
        assert_eq!(task.last_attempt_summary.as_deref(), Some("TOKEN_EXPIRED: 授权已过期"));
        error.summary = "长".repeat(1024);
        record_initial_failure(&mut task, &error).unwrap();
        assert_eq!(task.last_attempt_summary.as_ref().unwrap().chars().count(), 512);
        assert!(task.last_attempt_at.is_some());
    }

    type TestExecutor = ReadExecutor;

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
        let mut executor = TestExecutor::default();
        let port = RecordingWrite {
            identity: &mut executor as *mut TestExecutor as usize,
            fail_at: None,
            calls: Mutex::new(Vec::new()),
        };
        persist_failure(&port, &1, &2, &3, &mut executor).await.unwrap();
        assert_eq!(*port.calls.lock().unwrap(), ["error_task", "work_item", "audit"]);
    }

    #[tokio::test]
    async fn w29_failure_stops_at_each_write_preserving_first_error() {
        let expected = ["error_task", "work_item", "audit"];
        for fail_at in 0..expected.len() {
            let mut executor = TestExecutor::default();
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
        }
    }
}
