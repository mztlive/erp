//! 目录同步启动与结果事务；保留与健康检查不同的读取时点。
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::ports::supplier_api_gateway::ClassifiedError;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_api::context::digest;
use erp_support::{BackgroundJob, BulkJobExt, JobStatus};
use persistence_core::{NoTransaction, Transactional};

use super::SupplierConnectionExecutionProcess;
use super::execution::{ConnectionJobExecutionPort, execute};
use super::failure::persist_health_failure_task;
use crate::{Error, Result};

impl SupplierConnectionExecutionProcess {
    pub(super) async fn process_catalog_job(&self, job: BackgroundJob, actor: &AuditActor) -> Result<()> {
        execute(&CatalogExecution(self), job, actor).await
    }

    async fn start_background_job(&self, job: BackgroundJob) -> Result<BackgroundJob> {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .background_jobs()
                        .find_by_id(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("后台任务不存在".to_string()))?;
                    if job.status != JobStatus::Pending {
                        return Err(Error::ConflictError("后台任务已开始或已结束".to_string()));
                    }
                    job.start(Instant::now())?;
                    db.background_jobs().update(&mut job, session).await?;
                    Ok(job)
                })
            })
            .await
    }

    async fn finish_catalog_job(
        &self,
        job: BackgroundJob,
        connection: SupplierApiConnection,
        outcome: std::result::Result<(), ClassifiedError>,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .background_jobs()
                        .find_by_id(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("目录同步任务不存在".to_string()))?;
                    let at = Instant::now();
                    if let Err(error) = &outcome {
                        job.record_progress(0, 0, 1, at)?;
                        job.mark_failed(Some(format!("{}: {}", error.code, error.summary)), at)?;
                        persist_health_failure_task(&db, &connection, &job, error, &actor, session).await?;
                    } else {
                        job.record_progress(1, 0, 0, at)?;
                        job.mark_succeeded(at)?;
                    }
                    db.background_jobs().update(&mut job, session).await?;
                    let audit = actor.clone().resource_log_with_id(
                        format!("w20-catalog-audit-{}", digest(&[&job.base.id])),
                        "supplier_api_connection.catalog_sync.settle",
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

/// 生产适配器：连接在启动事务之前读取，结果事务不增加配置复验。
struct CatalogExecution<'a>(&'a SupplierConnectionExecutionProcess);
impl ConnectionJobExecutionPort for CatalogExecution<'_> {
    type Job = BackgroundJob;
    type Started = (SupplierApiConnection, BackgroundJob);
    type Outcome = std::result::Result<(), ClassifiedError>;
    async fn start(&self, job: Self::Job) -> Result<Self::Started> {
        let connection_id = job
            .domain_job_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("目录同步任务缺少连接ID".to_string()))?;
        let connection = SupplierApiService::new(self.0.db.clone())
            .load_connection(&connection_id, &mut NoTransaction)
            .await?;
        let started_job = self.0.start_background_job(job).await?;
        Ok((connection, started_job))
    }
    async fn invoke(&self, started: &Self::Started) -> Self::Outcome {
        self.0.gateway.catalog_sync(&started.0).await
    }
    async fn finish(&self, started: Self::Started, outcome: Self::Outcome, actor: &AuditActor) -> Result<()> {
        self.0.finish_catalog_job(started.1, started.0, outcome, actor).await
    }
}
