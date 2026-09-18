//! Named source-registry processes that own audited outer transactions.

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_support::{
    CreateSourceSystemRequest, SourceRegistryExt, SourceSystem, SourceSystemId, SourceSystemView,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::Result;
use crate::audit::run_audited;

/// Process module name.
pub fn process_name() -> &'static str {
    "source_registry"
}

/// Create a source system and persist the success audit in one transaction.
pub async fn create_source_system(
    db: Database,
    req: CreateSourceSystemRequest,
    actor: AuditActor,
) -> Result<SourceSystemView> {
    req.validate()?;
    let id = SourceSystemId::new(next_id());
    let system = SourceSystem::new(id, req.into_data(), actor.id())?;
    let audit =
        actor.clone().resource_log("source_system.create", "source_system", system.base.id.clone())?;
    let system_for_tx = system.clone();
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            db.source_systems().create(&system_for_tx, executor).await?;
            Ok(())
        })
    })
    .await?;
    Ok(system.into())
}
