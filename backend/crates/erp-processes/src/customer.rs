//! Named customer processes that own audited outer transactions.

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_customer::CustomerExt;
use mongodb::Database;
use services::Result;

use crate::adapters::customer_service;
use crate::audit::run_audited;

/// Process module name.
pub fn process_name() -> &'static str {
    "customer"
}

/// Soft-delete a customer role and persist the success audit in one transaction.
pub async fn delete_customer(db: Database, id: String, actor: AuditActor) -> Result<()> {
    let mut account = customer_service(db.clone()).load_customer(&id).await?;
    let audit = actor
        .clone()
        .resource_log("customer.delete", "customer", account.base.id.clone())?;
    run_audited(&db, audit, move |db, session| {
        Box::pin(async move {
            db.customer_accounts().soft_delete(&mut account, session).await?;
            Ok(())
        })
    })
    .await?;
    Ok(())
}
