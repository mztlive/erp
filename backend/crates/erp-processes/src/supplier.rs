//! Named supplier processes that own audited outer transactions.

use application_core::AuditActor;
use database::SupplierExt;
use erp_audit::AuditActorLogs;
use mongodb::Database;
use services::supplier::SupplierService;
use services::Result;

use crate::audit::run_audited;

/// Process module name.
pub fn process_name() -> &'static str {
    "supplier"
}

/// Soft-delete a supplier role and persist the success audit in one transaction.
pub async fn delete_supplier(db: Database, id: String, actor: AuditActor) -> Result<()> {
    let mut supplier = SupplierService::new(db.clone()).load_supplier(&id).await?;
    let audit = actor
        .clone()
        .resource_log("supplier.delete", "supplier", supplier.base.id.clone())?;
    run_audited(&db, audit, move |db, session| {
        Box::pin(async move {
            db.supplier_accounts().soft_delete(&mut supplier, session).await?;
            Ok(())
        })
    })
    .await?;
    Ok(())
}
