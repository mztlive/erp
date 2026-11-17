use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::{internal, supplier_deposit};
use erp_read_models::Error as ReadError;
use erp_read_models::purchase_center::dto::PurchaseOrderCenterView;
use erp_supplier_orders::supplier_deposits::SupplierDepositReadAccess;
use persistence_core::NoTransaction;

use super::purchase_reads;
use crate::http::errors::Error;
use crate::http::state::HttpState;

pub(super) async fn attach(
    state: &HttpState,
    actor: &AuditActor,
    view: &mut PurchaseOrderCenterView,
) -> Result<(), Error> {
    for permission in [&supplier_deposit::DETAIL, &internal::SUPPLIER_OFFERING_COST_DETAIL] {
        if !state.application().authorization().has_permission(actor, permission, &mut NoTransaction).await? {
            return Ok(());
        }
    }
    let access = Arc::new(SupplierDepositReadAccess::new(
        state.application().db(),
        state.application().authorization(),
    ));
    match purchase_reads(state).purchase_order_deposit_funds(&view.id, view.version, actor, access).await {
        Ok(funds) => view.api_execution_funds = Some(funds),
        Err(ReadError::Forbidden(_)) => {},
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
