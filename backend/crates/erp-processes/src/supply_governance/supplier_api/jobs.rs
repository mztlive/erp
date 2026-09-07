use super::{
    receipt::{persist_command_receipt, CommandReceiptWrite},
    SupplierApiGovernanceProcess,
};
use application_core::AuditActor;
use erp_core::ids::BackgroundJobId;
use erp_supply::{
    dto::supplier_api::SupplierConnectionCommandResult,
    entity::supplier_api::{SupplierCommandOutcome, SupplierConnectionAction, SupplierHealthCheckType},
    service::supplier_api::{command::CommandIdentity, SupplierApiService},
};
use erp_support::{BackgroundJob, BulkJobExt, SupplierGovernanceJobKind, SupplierGovernanceJobSpec};
use id_generator::next_id;
use persistence_core::Transactional;
use services::Result;
impl SupplierApiGovernanceProcess {
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
                    let domain = SupplierApiService::new(db.clone());
                    let (connection, capabilities) = domain
                        .prepare_job_target(
                            &connection_id_value,
                            expected_version,
                            SupplierConnectionAction::RunHealthCheck,
                            session,
                        )
                        .await?;
                    let job = BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
                        job_id: BackgroundJobId::new(next_id()),
                        connection_id: connection_id_value.clone(),
                        kind: SupplierGovernanceJobKind::HealthCheck,
                        requested_by: actor.id().to_string(),
                        idempotency_hash: identity.idempotency_hash.clone(),
                    })?;
                    let run = SupplierApiService::prepare_health_run(
                        &connection,
                        &capabilities,
                        job.base.id.clone(),
                        check_type,
                        actor.id(),
                        &identity,
                    )?;
                    db.background_jobs().create(&job, session).await?;
                    domain.create_health_run(&run, session).await?;
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
                    let domain = SupplierApiService::new(db.clone());
                    let (connection, _capabilities) = domain
                        .prepare_job_target(
                            &connection_id_value,
                            expected_version,
                            SupplierConnectionAction::StartCatalogSync,
                            session,
                        )
                        .await?;
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
