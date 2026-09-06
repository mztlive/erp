use std::time::Instant as MonotonicInstant;

use database::{AccessControlExt, BulkJobExt, IntegrationOpsExt, SupplierApiExt, WorkItemExt};
use entities::bulk_job::{
    BackgroundJob, JobStatus, SupplierGovernanceJobKind, SupplierGovernanceJobSpec,
    SUPPLIER_CATALOG_SYNC_JOB_TYPE, SUPPLIER_HEALTH_CHECK_JOB_TYPE,
};
use entities::integration_ops::{ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData};
use entities::supplier_api::{
    CapabilityVersionSnapshot, HealthCheckResult, SupplierApiConnection, SupplierCommandOutcome,
    SupplierConnectionAction, SupplierConnectionGovernance, SupplierHealthCheckRun,
    SupplierHealthCheckRunData, SupplierHealthCheckType,
};
use erp_core::common::time::Instant;
use erp_core::ids::{BackgroundJobId, IntegrationErrorTaskId, SupplierApiConnectionId};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};

use crate::audit::AuditActorLogs;
use crate::errors::{Error, Result};
use crate::integration_ops::{error_owner_role, error_work_item};
use application_core::AuditActor;

use super::super::dto::{SupplierConnectionCommandResult, SupplierConnectionJobView};
use super::super::{ClassifiedError, SupplierApiService};
use super::command::{persist_command_receipt, CommandIdentity, CommandReceiptWrite};
use super::context::{digest, ensure_version};

impl SupplierApiService {
    /// 查询连接下健康检查或目录同步后台任务的当前终态/进度。
    ///
    /// # Errors
    /// 任务不存在或不属于指定连接时返回 `NotFound`。
    pub async fn connection_job(
        &self,
        connection_id: &str,
        job_id: &str,
    ) -> Result<SupplierConnectionJobView> {
        let job = self
            .db
            .supplier_api()
            .connection_job(
                &SupplierApiConnectionId::new(connection_id),
                job_id,
                &[SUPPLIER_HEALTH_CHECK_JOB_TYPE, SUPPLIER_CATALOG_SYNC_JOB_TYPE],
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("连接后台任务不存在".to_string()))?;
        Ok(job_view(job))
    }

    /// 执行已登记的连接后台任务。
    ///
    /// 该入口供 Web 进程后台调度器调用，绝不能在创建任务的 HTTP 请求内等待。
    /// 默认未注入真实 adapter 时任务形成明确失败终态并进入 W29。
    ///
    /// # Errors
    /// 任务不存在、状态冲突或任务结果落库失败时返回错误。
    pub async fn process_connection_job(&self, job_id: &str, actor: &AuditActor) -> Result<()> {
        let job = self
            .db
            .supplier_api()
            .governance_job(job_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("连接后台任务不存在".to_string()))?;
        if job.is_terminal() {
            return Ok(());
        }
        match job.domain_job_type.as_deref() {
            Some(SUPPLIER_HEALTH_CHECK_JOB_TYPE) => self.process_health_job(job, actor).await,
            Some(SUPPLIER_CATALOG_SYNC_JOB_TYPE) => self.process_catalog_job(job, actor).await,
            _ => Err(Error::BusinessLogicError("任务不属于 W20 连接治理".to_string())),
        }
    }

    pub(super) async fn create_health_job(
        &self,
        id: &str,
        check_type: SupplierHealthCheckType,
        expected_version: u64,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, expected_version)?;
                    let capabilities = db
                        .supplier_api()
                        .connection_capabilities(
                            &SupplierApiConnectionId::new(connection_id_value.clone()),
                            session,
                        )
                        .await?;
                    let governance = SupplierConnectionGovernance {
                        connection: &connection,
                        capabilities: &capabilities,
                        confirmations: &[],
                        health_runs: &[],
                    };
                    if let Some(blocker) = governance
                        .blockers(SupplierConnectionAction::RunHealthCheck, Default::default(), true)
                        .first()
                    {
                        return Err(Error::BusinessLogicError(blocker.message.clone()));
                    }
                    let job = BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
                        job_id: BackgroundJobId::new(next_id()),
                        connection_id: connection_id_value.clone(),
                        kind: SupplierGovernanceJobKind::HealthCheck,
                        requested_by: actor.id().to_string(),
                        idempotency_hash: identity.idempotency_hash.clone(),
                    })?;
                    let run = SupplierHealthCheckRun::new(
                        format!("w20-health-{}", digest(&[&job.base.id])),
                        SupplierHealthCheckRunData {
                            connection_id: SupplierApiConnectionId::new(connection_id_value.clone()),
                            background_job_id: job.base.id.clone(),
                            check_type,
                            technical_config_version: connection.technical_config_version,
                            capability_versions: capabilities
                                .iter()
                                .filter(|capability| capability.status.is_active())
                                .map(|capability| CapabilityVersionSnapshot {
                                    capability_code: capability.capability_code,
                                    version: capability.base.version,
                                })
                                .collect(),
                            requested_by: actor.id().to_string(),
                            idempotency_key_hash: identity.idempotency_hash.clone(),
                            request_fingerprint: identity.fingerprint.clone(),
                        },
                    )?;
                    db.background_jobs().create(&job, session).await?;
                    db.supplier_api_health_check_runs().create(&run, session).await?;
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action: SupplierConnectionAction::RunHealthCheck,
                            identity: &identity,
                            outcome: SupplierCommandOutcome::Processing,
                            job_id: Some(job.base.id.clone()),
                            actor: &actor,
                        },
                        session,
                    )
                    .await
                })
            })
            .await
    }

    pub(super) async fn create_catalog_job(
        &self,
        id: &str,
        expected_version: u64,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, expected_version)?;
                    let capabilities = db
                        .supplier_api()
                        .connection_capabilities(
                            &SupplierApiConnectionId::new(connection_id_value.clone()),
                            session,
                        )
                        .await?;
                    let governance = SupplierConnectionGovernance {
                        connection: &connection,
                        capabilities: &capabilities,
                        confirmations: &[],
                        health_runs: &[],
                    };
                    let blockers = governance.blockers(
                        SupplierConnectionAction::StartCatalogSync,
                        Default::default(),
                        true,
                    );
                    if let Some(blocker) = blockers.first() {
                        return Err(Error::BusinessLogicError(blocker.message.clone()));
                    }
                    let job = BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
                        job_id: BackgroundJobId::new(next_id()),
                        connection_id: connection_id_value.clone(),
                        kind: SupplierGovernanceJobKind::CatalogSync,
                        requested_by: actor.id().to_string(),
                        idempotency_hash: identity.idempotency_hash.clone(),
                    })?;
                    db.background_jobs().create(&job, session).await?;
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action: SupplierConnectionAction::StartCatalogSync,
                            identity: &identity,
                            outcome: SupplierCommandOutcome::Processing,
                            job_id: Some(job.base.id.clone()),
                            actor: &actor,
                        },
                        session,
                    )
                    .await
                })
            })
            .await
    }

    async fn process_health_job(&self, job: BackgroundJob, actor: &AuditActor) -> Result<()> {
        let (connection, started_job, started_run) = self.start_health_job(job).await?;
        let started = MonotonicInstant::now();
        let outcome = self.gateway.health_check(&connection).await;
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.finish_health_job(started_job, started_run, outcome, latency_ms, actor)
            .await
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

    async fn process_catalog_job(&self, job: BackgroundJob, actor: &AuditActor) -> Result<()> {
        let connection_id = job
            .domain_job_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("目录同步任务缺少连接ID".to_string()))?;
        let connection = self.load_connection(&connection_id, &mut NoTransaction).await?;
        let started_job = self.start_background_job(job).await?;
        let outcome = self.gateway.catalog_sync(&connection).await;
        self.finish_catalog_job(started_job, connection, outcome, actor)
            .await
    }

    async fn start_background_job(&self, job: BackgroundJob) -> Result<BackgroundJob> {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .supplier_api()
                        .governance_job(&job.base.id, session)
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
                        .supplier_api()
                        .governance_job(&job.base.id, session)
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

fn job_view(job: BackgroundJob) -> SupplierConnectionJobView {
    SupplierConnectionJobView {
        job_id: job.base.id,
        job_no: job.job_no,
        action: job.domain_job_type.unwrap_or_default(),
        status: job.status,
        total: job.total_count,
        processed: job.processed_count,
        succeeded: job.success_count,
        failed: job.failed_count,
        error_summary: job.error_summary,
        created_at: job.base.created_at,
        finished_at: job.finished_at.map(|at| at.unix_secs() as u64),
    }
}

fn settle_health_failure(
    job: &mut BackgroundJob,
    run: &mut SupplierHealthCheckRun,
    at: Instant,
    latency_ms: u64,
    error: &ClassifiedError,
) -> Result<()> {
    job.record_progress(0, 0, 1, at)?;
    job.mark_failed(Some(format!("{}: {}", error.code, error.summary)), at)?;
    if error.class == ErrorClass::ResultUnknown {
        run.mark_unknown(at, latency_ms, error.code.clone(), error.summary.clone())?;
    } else {
        run.fail(at, latency_ms, error.code.clone(), error.summary.clone())?;
    }
    Ok(())
}

async fn persist_health_failure_task(
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
            error_class: error.class,
            owner_role: Some(error_owner_role(error.class).to_string()),
            owner_user_id: Some(actor.id().to_string()),
        },
    )?;
    let work_item = error_work_item(&task, actor.id())?;
    let work_item_audit = actor.clone().resource_log_with_id(
        format!("w20-work-audit-{}", digest(&[&job.base.id])),
        "integration_error_task.work_item.create",
        "work_item",
        work_item.base.id.clone(),
        Some(format!("job_id={}", job.base.id)),
    )?;
    db.integration_error_tasks().create(&task, executor).await?;
    db.work_items().create(&work_item, executor).await?;
    db.audit_logs().create(&work_item_audit, executor).await?;
    Ok(())
}
