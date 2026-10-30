use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::Permission;
use erp_processes::supplier_deposits::SupplierDepositReadAccess;
use erp_read_models::Error as ReadError;
use erp_read_models::purchase_center::dto::PurchaseOrderCenterView;

use super::purchase_reads;
use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::middleware::RbacSubject;

pub(super) async fn attach(
    state: &AppState,
    subject: &RbacSubject,
    actor: &AuditActor,
    view: &mut PurchaseOrderCenterView,
) -> Result<(), Error> {
    for code in ["supplier_deposit:detail", "supplier_offering_cost:detail"] {
        if !state.rbac().enforce(&subject.0, &Permission::parse(code)?).await? {
            return Ok(());
        }
    }
    let access = Arc::new(SupplierDepositReadAccess::new(state.db(), state.rbac()));
    match purchase_reads(state).purchase_order_deposit_funds(&view.id, view.version, actor, access).await {
        Ok(funds) => view.api_execution_funds = Some(funds),
        Err(ReadError::Forbidden(_)) => {},
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
