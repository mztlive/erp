//! Named party processes that own audited outer transactions.

use application_core::AuditActor;
use database::PartyExt;
use erp_audit::AuditActorLogs;
use mongodb::Database;
use services::party::PartyService;
use services::Result;

use crate::audit::run_audited;

/// Named party process.
pub fn process_name() -> &'static str {
    "party"
}

/// Soft-delete a party and persist the success audit in one transaction.
pub async fn delete_party(db: Database, id: String, actor: AuditActor) -> Result<()> {
    let mut party = PartyService::new(db.clone()).load_party(&id).await?;
    services::party::ensure_outside_supplier_profile(&db, &erp_core::ids::PartyId::new(&id)).await?;
    let audit = actor
        .clone()
        .resource_log("party.delete", "party", party.base.id.clone())?;
    run_audited(&db, audit, move |db, session| {
        Box::pin(async move {
            db.parties().soft_delete(&mut party, session).await?;
            Ok(())
        })
    })
    .await?;
    Ok(())
}
