//! 健康检查启动与结果事务；外部调用位于两个事务之间。
use super::{
    execution::{execute, ConnectionJobExecutionPort},
    failure::{persist_health_failure_task, settle_health_failure},
    SupplierConnectionExecutionProcess,
};
use application_core::AuditActor;
use database::SupplierApiExt;
use entities::supplier_api::{HealthCheckResult, SupplierApiConnection, SupplierHealthCheckRun};
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_integration::entity::integration_ops::ErrorClass;
use erp_support::{BackgroundJob, BulkJobExt, JobStatus};
use persistence_core::Transactional;
use services::supplier_api::{digest, ClassifiedError};
use services::{Error, Result};
use std::time::Instant as MonotonicInstant;

impl SupplierConnectionExecutionProcess {
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
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .supplier_api()
                        .governance_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查任务不存在".to_string()))?;
                    if job.status != JobStatus::Pending {
                        return Err(Error::ConflictError("健康检查任务已开始或已结束".to_string()));
                    }
                    let mut run = db
                        .supplier_api()
                        .health_run_for_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查运行记录不存在".to_string()))?;
                    let connection = db
                        .supplier_api()
                        .connection(&run.connection_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    let at = Instant::now();
                    job.start(at)?;
                    run.start(at)?;
                    db.background_jobs().update(&mut job, session).await?;
                    db.supplier_api_health_check_runs()
                        .update(&mut run, session)
                        .await?;
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
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .supplier_api()
                        .governance_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查任务不存在".to_string()))?;
                    let mut run = db
                        .supplier_api()
                        .health_run_for_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查运行记录不存在".to_string()))?;
                    let mut connection = db
                        .supplier_api()
                        .connection(&run.connection_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    let at = Instant::now();
                    let config_changed = connection.technical_config_version != run.technical_config_version;
                    if config_changed {
                        let error = ClassifiedError {
                            class: ErrorClass::ResultUnknown,
                            code: "TECHNICAL_CONFIG_CHANGED".to_string(),
                            summary: "检查期间技术配置已变化，本次结果不能作为启用依据".to_string(),
                        };
                        settle_health_failure(&mut job, &mut run, at, latency_ms, &error)?;
                        persist_health_failure_task(&db, &connection, &job, &error, &actor, session).await?;
                    } else if let Err(error) = &outcome {
                        settle_health_failure(&mut job, &mut run, at, latency_ms, error)?;
                        connection.record_health(HealthCheckResult::Failed, at);
                        connection.stable.touch(actor.id());
                        db.supplier_api_connections()
                            .update(&mut connection, session)
                            .await?;
                        persist_health_failure_task(&db, &connection, &job, error, &actor, session).await?;
                    } else {
                        job.record_progress(1, 0, 0, at)?;
                        job.mark_succeeded(at)?;
                        run.succeed(at, latency_ms)?;
                        connection.record_health(HealthCheckResult::Healthy, at);
                        connection.stable.touch(actor.id());
                        db.supplier_api_connections()
                            .update(&mut connection, session)
                            .await?;
                    }
                    db.background_jobs().update(&mut job, session).await?;
                    db.supplier_api_health_check_runs()
                        .update(&mut run, session)
                        .await?;
                    let audit = actor.clone().resource_log_with_id(
                        format!("w20-health-audit-{}", digest(&[&job.base.id])),
                        "supplier_api_connection.health_check.settle",
                        "supplier_api_connection",
                        connection.base.id,
                        Some(format!("job_id={};status={}", job.base.id, job.status.as_str())),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(())
                })
            })
            .await
    }
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
        let started_at = MonotonicInstant::now();
        let outcome = self.0.gateway.health_check(&started.0).await;
        let latency_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        (outcome, latency_ms)
    }
    async fn finish(&self, started: Self::Started, outcome: Self::Outcome, actor: &AuditActor) -> Result<()> {
        self.0
            .finish_health_job(started.1, started.2, outcome.0, outcome.1, actor)
            .await
    }
}
