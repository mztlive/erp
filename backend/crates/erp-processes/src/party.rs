//! Named party processes that own audited outer transactions.

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::ids::PartyId;
use erp_party::PartyExt;
use mongodb::Database;
use services::Result;

use crate::adapters::{party_service, MongoSupplierRole};
use crate::audit::run_audited;

/// Named party process.
pub fn process_name() -> &'static str {
    "party"
}

/// Soft-delete a party and persist the success audit in one transaction.
pub async fn delete_party(db: Database, id: String, actor: AuditActor) -> Result<()> {
    let mut party = party_service(db.clone()).load_party(&id).await?;
    erp_party::ensure_outside_supplier_profile(&*MongoSupplierRole::shared(db.clone()), &PartyId::new(&id))
        .await?;
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
