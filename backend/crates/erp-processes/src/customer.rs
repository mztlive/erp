//! Named customer processes that own audited outer transactions.

use application_core::AuditActor;
use database::CustomerExt;
use erp_audit::AuditActorLogs;
use mongodb::Database;
use services::customer::CustomerService;
use services::Result;

use crate::audit::run_audited;

/// Process module name.
pub fn process_name() -> &'static str {
    "customer"
}

/// Soft-delete a customer role and persist the success audit in one transaction.
pub async fn delete_customer(db: Database, id: String, actor: AuditActor) -> Result<()> {
    let mut account = CustomerService::new(db.clone()).load_customer(&id).await?;
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
