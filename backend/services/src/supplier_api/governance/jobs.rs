use super::super::dto::{SupplierConnectionCommandResult, SupplierConnectionJobView};
use super::super::SupplierApiService;
use super::command::{persist_command_receipt, CommandIdentity, CommandReceiptWrite};
use super::context::{digest, ensure_version};
use crate::errors::{Error, Result};
use application_core::AuditActor;
use database::SupplierApiExt;
use entities::supplier_api::{
    CapabilityVersionSnapshot, SupplierCommandOutcome, SupplierConnectionAction,
    SupplierConnectionGovernance, SupplierHealthCheckRun, SupplierHealthCheckRunData,
    SupplierHealthCheckType,
};
use erp_core::ids::{BackgroundJobId, SupplierApiConnectionId};
use erp_support::BulkJobExt;
use erp_support::{
    BackgroundJob, SupplierGovernanceJobKind, SupplierGovernanceJobSpec, SUPPLIER_CATALOG_SYNC_JOB_TYPE,
    SUPPLIER_HEALTH_CHECK_JOB_TYPE,
};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};

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
