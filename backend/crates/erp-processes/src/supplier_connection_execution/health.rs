//! 健康检查启动与结果事务；外部调用位于两个事务之间。
use std::time::Instant as MonotonicInstant;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::{HealthCheckResult, SupplierApiConnection, SupplierHealthCheckRun};
use erp_supply::ports::supplier_api_gateway::{ClassifiedError, SupplierApiGateway};
use erp_supply::repository::SupplierApiExt;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_api::context::digest;
use erp_support::{BackgroundJob, BulkJobExt, JobStatus};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::SupplierConnectionExecutionProcess;
use super::execution::{ConnectionJobExecutionPort, execute};
use super::failure::{persist_health_failure_task, settle_health_failure};
use crate::audit::persist_log;
use crate::{Error, Result};

impl SupplierConnectionExecutionProcess {
    /// 按启动事务、事务外健康检查、结果事务的顺序执行健康检查任务。
    ///
    /// # 参数
    /// * `job` - 已登记的健康检查后台任务。
    /// * `actor` - 结果事务使用的审计操作人。
    ///
    /// # 返回
    /// 启动与结果事务都成功时返回；网关失败由结果事务登记。
    ///
    /// # 错误
    /// 启动或结果事务失败时返回错误。
    pub(super) async fn process_health_job(&self, job: BackgroundJob, actor: &AuditActor) -> Result<()> {
        execute(&HealthExecution(self), job, actor).await
    }

    async fn start_health_job(
        &self,
        job: BackgroundJob,
    ) -> Result<(SupplierApiConnection, BackgroundJob, SupplierHealthCheckRun)> {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut job = db
                        .background_jobs()
                        .find_by_id(&job.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查任务不存在".to_string()))?;
                    if job.status != JobStatus::Pending {
                        return Err(Error::ConflictError("健康检查任务已开始或已结束".to_string()));
                    }
                    let mut run = db
                        .supplier_api()
                        .health_run_for_job(&job.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查运行记录不存在".to_string()))?;
                    let connection = db
                        .supplier_api()
                        .connection(&run.connection_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    let at = Instant::now();
                    job.start(at)?;
                    run.start(at)?;
                    db.background_jobs().update(&mut job, executor).await?;
                    SupplierApiService::new(db.clone()).persist_health_run(&mut run, executor).await?;
                    Ok((connection, job, run))
                })
            })
            .await
    }

    async fn finish_health_job(
        &self,
        job: BackgroundJob,
        _run: SupplierHealthCheckRun,
        outcome: std::result::Result<(), ClassifiedError>,
        latency_ms: u64,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut job = db
                        .background_jobs()
                        .find_by_id(&job.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查任务不存在".to_string()))?;
                    let mut run = db
                        .supplier_api()
                        .health_run_for_job(&job.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查运行记录不存在".to_string()))?;
                    let mut connection = db
                        .supplier_api()
                        .connection(&run.connection_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    let at = Instant::now();
                    let config_changed = connection.technical_config_version != run.technical_config_version;
                    let service = SupplierApiService::new(db.clone());
                    if config_changed {
                        let error = ClassifiedError {
                            class: SupplierFailureClass::ResultUnknown,
                            code: "TECHNICAL_CONFIG_CHANGED".to_string(),
                            summary: "检查期间技术配置已变化，本次结果不能作为启用依据".to_string(),
                        };
                        settle_health_failure(&mut job, &mut run, at, latency_ms, &error)?;
                        persist_health_failure_task(&db, &connection, &job, &error, &actor, executor).await?;
                    } else if let Err(error) = &outcome {
                        settle_health_failure(&mut job, &mut run, at, latency_ms, error)?;
                        connection.record_health(HealthCheckResult::Failed, at);
                        connection.stable.touch(actor.id());
                        service.persist_connection(&mut connection, executor).await?;
                        persist_health_failure_task(&db, &connection, &job, error, &actor, executor).await?;
                    } else {
                        job.record_progress(1, 0, 0, at)?;
                        job.mark_succeeded(at)?;
                        run.succeed(at, latency_ms)?;
                        connection.record_health(HealthCheckResult::Healthy, at);
                        connection.stable.touch(actor.id());
                        service.persist_connection(&mut connection, executor).await?;
                    }
                    persist_health_result(&db, connection, &mut job, &mut run, &actor, executor).await
                })
            })
            .await
    }
}

async fn persist_health_result(
    db: &Database,
    connection: SupplierApiConnection,
    job: &mut BackgroundJob,
    run: &mut SupplierHealthCheckRun,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.background_jobs().update(job, executor).await?;
    SupplierApiService::new(db.clone()).persist_health_run(run, executor).await?;
    let audit = actor.clone().resource_log_with_id(
        format!("w20-health-audit-{}", digest(&[&job.base.id])),
        "supplier_api_connection.health_check.settle",
        "supplier_api_connection",
        connection.base.id,
        Some(format!("job_id={};status={}", job.base.id, job.status.as_str())),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 生产适配器：start/finish 各自完成根事务，invoke 只持有已提交事实。
struct HealthExecution<'a>(&'a SupplierConnectionExecutionProcess);
impl ConnectionJobExecutionPort for HealthExecution<'_> {
    type Job = BackgroundJob;
    type Started = (SupplierApiConnection, BackgroundJob, SupplierHealthCheckRun);
    type Outcome = (std::result::Result<(), ClassifiedError>, u64);
    async fn start(&self, job: Self::Job) -> Result<Self::Started> {
        self.0.start_health_job(job).await
    }
    async fn invoke(&self, started: &Self::Started) -> Self::Outcome {
        invoke_health_check(self.0.gateway.as_ref(), started).await
    }
    async fn finish(&self, started: Self::Started, outcome: Self::Outcome, actor: &AuditActor) -> Result<()> {
        self.0.finish_health_job(started.1, started.2, outcome.0, outcome.1, actor).await
    }
}

async fn invoke_health_check(
    gateway: &dyn SupplierApiGateway,
    started: &(SupplierApiConnection, BackgroundJob, SupplierHealthCheckRun),
) -> (std::result::Result<(), ClassifiedError>, u64) {
    let started_at = MonotonicInstant::now();
    let outcome = gateway.health_check(&started.0, started.2.check_type).await;
    let latency_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
    (outcome, latency_ms)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    use erp_core::ids::{BackgroundJobId, SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::supplier_api::{
        ConnectionEnvironment, SupplierApiConnectionData, SupplierApiConnectionStatus,
        SupplierHealthCheckRunData, SupplierHealthCheckStatus, SupplierHealthCheckType,
    };
    use erp_support::{SupplierGovernanceJobKind, SupplierGovernanceJobSpec};

    use super::*;

    struct TypedHealthGateway(Mutex<Vec<(String, SupplierHealthCheckType)>>);
    impl SupplierApiGateway for TypedHealthGateway {
        fn health_check<'a>(
            &'a self,
            connection: &'a SupplierApiConnection,
            check_type: SupplierHealthCheckType,
        ) -> Pin<Box<dyn Future<Output = std::result::Result<(), ClassifiedError>> + Send + 'a>> {
            Box::pin(async move {
                self.0.lock().unwrap().push((connection.base.id.clone(), check_type));
                if check_type == SupplierHealthCheckType::CapabilityMetadata {
                    return Err(ClassifiedError {
                        class: SupplierFailureClass::CapabilityGap,
                        code: "DGSS_CAPABILITY_METADATA_UNSUPPORTED".into(),
                        summary: "能力元数据未核验".into(),
                    });
                }
                Ok(())
            })
        }
    }

    fn started(
        check_type: SupplierHealthCheckType,
    ) -> (SupplierApiConnection, BackgroundJob, SupplierHealthCheckRun) {
        let connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-1"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                connection_code: "dgss-test".into(),
                environment: ConnectionEnvironment::Testing,
                endpoint_reference: "unbound".into(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "actor",
        )
        .unwrap();
        let mut job = BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
            job_id: BackgroundJobId::new("job-1"),
            connection_id: "conn-1".into(),
            kind: SupplierGovernanceJobKind::HealthCheck,
            requested_by: "actor".into(),
            idempotency_hash: "1234567890abcdef".into(),
        })
        .unwrap();
        let mut run = SupplierHealthCheckRun::new(
            "run-1",
            SupplierHealthCheckRunData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                background_job_id: job.base.id.clone(),
                check_type,
                technical_config_version: connection.technical_config_version,
                capability_versions: vec![],
                requested_by: "actor".into(),
                idempotency_key_hash: "1234567890abcdef".into(),
                request_fingerprint: "1234567890abcdef".into(),
            },
        )
        .unwrap();
        job.start(Instant::now()).unwrap();
        run.start(Instant::now()).unwrap();
        (connection, job, run)
    }

    #[tokio::test]
    async fn health_invoke_preserves_started_check_type_and_metadata_failure_is_not_success() {
        let gateway = TypedHealthGateway(Mutex::new(vec![]));
        for check_type in [
            SupplierHealthCheckType::Connectivity,
            SupplierHealthCheckType::Authentication,
            SupplierHealthCheckType::CapabilityMetadata,
        ] {
            let mut started = started(check_type);
            let (outcome, latency_ms) = invoke_health_check(&gateway, &started).await;
            if check_type == SupplierHealthCheckType::CapabilityMetadata {
                let error = outcome.unwrap_err();
                assert_eq!(error.class, SupplierFailureClass::CapabilityGap);
                assert_eq!(error.code, "DGSS_CAPABILITY_METADATA_UNSUPPORTED");
                settle_health_failure(&mut started.1, &mut started.2, Instant::now(), latency_ms, &error)
                    .unwrap();
                assert_eq!(started.1.status, JobStatus::Failed);
                assert_eq!(started.2.status, SupplierHealthCheckStatus::Failed);
                assert_ne!(started.0.last_health_result, Some(HealthCheckResult::Healthy));
            } else {
                assert!(outcome.is_ok());
            }
        }
        assert_eq!(
            *gateway.0.lock().unwrap(),
            [
                ("conn-1".into(), SupplierHealthCheckType::Connectivity),
                ("conn-1".into(), SupplierHealthCheckType::Authentication),
                ("conn-1".into(), SupplierHealthCheckType::CapabilityMetadata),
            ]
        );
    }
}
