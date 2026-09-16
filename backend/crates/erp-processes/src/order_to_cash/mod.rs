//! Sales lifecycle, approval and financial formalization processes.

mod adapter;
pub mod adapters;
mod authorization;
mod cancel_approval;
mod command;
mod draft_working_copy;
mod formalization_posting;
mod formalization_root;
mod formalize;
mod handover;
mod procurement;
pub mod progress;
mod start_approval;

pub use adapter::sales_order_object_readable;
use erp_identity::SharedRbacService;
use erp_sales::entity::sales_order::BusinessType;
use erp_sales::service::sales_order::SalesOrderService;
use erp_workflow::entity::document_registry::DocumentType;
pub use formalization_root::SalesOrderFormalizationProcess;
use formalize::FormalizedSubmissionWrite;
use mongodb::Database;

use crate::{Error, Result};

/// Sales lifecycle commands combining sales writes, provider checks, workflow and audit.
pub struct SalesOrderCommandProcess {
    db: Database,
    rbac: Option<SharedRbacService>,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
}
impl SalesOrderCommandProcess {
    /// Construct with fail-closed approval binding defaults; performs no I/O.
    pub fn new(db: Database) -> Self {
        Self { db, rbac: None, object_read: std::sync::Arc::new(erp_workflow::FailClosedObjectReadPort) }
    }
    /// Construct with an authorization source while retaining fail-closed object-read defaults.
    pub fn with_rbac(db: Database, rbac: SharedRbacService) -> Self {
        Self { rbac: Some(rbac), ..Self::new(db) }
    }
    /// Inject the composition root's object-read provider for approval binding.
    pub fn with_object_read(
        mut self,
        port: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    ) -> Self {
        self.object_read = port;
        self
    }
    fn require_rbac(&self) -> Result<&SharedRbacService> {
        self.rbac.as_ref().ok_or_else(|| Error::Internal("销售单审批绑定需要授权源".into()))
    }
    fn sales(&self) -> SalesOrderService {
        SalesOrderService::new(self.db.clone())
    }
    fn read_model(&self) -> erp_read_models::sales_center::order::SalesOrderReadService {
        match &self.rbac {
            Some(rbac) => erp_read_models::sales_center::order::SalesOrderReadService::with_rbac(
                self.db.clone(),
                rbac.clone(),
            ),
            None => erp_read_models::sales_center::order::SalesOrderReadService::new(self.db.clone()),
        }
    }
    fn catalog(&self) -> adapters::catalog::CatalogQualificationAdapter {
        adapters::catalog::CatalogQualificationAdapter::new(self.db.clone())
    }
}
fn document_type_of_sales_business(business_type: BusinessType) -> DocumentType {
    match business_type {
        BusinessType::GoodsService => DocumentType::SalesOrder,
        BusinessType::Voucher => DocumentType::VoucherSalesOrder,
    }
}
fn subject_ref_for_sales_business(business_type: BusinessType, id: &str) -> Result<bpm::SubjectRef> {
    erp_workflow::entity::approval_integration::subject_ref_for(
        document_type_of_sales_business(business_type),
        id,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

/// Cancel the sales approval state within the workflow runtime's existing transaction.
///
/// The workflow action is checked against the sales type before sales writes and audit.
pub async fn cancel_approval_in_transaction(
    db: &Database,
    id: &str,
    action: erp_workflow::service::approval::policy::ApprovalDomainAction,
    actor: &application_core::AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    use erp_audit::{AuditActorLogs, AuditExt};
    use erp_sales::repository::SalesOrderExt;
    let mut order = db
        .sales_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".into()))?;
    adapter::execute_sales_order_domain_action(&mut order, action, actor.id())?;
    SalesOrderService::new(db.clone()).persist_order(&mut order, executor).await?;
    let audit = actor.clone().resource_log("sales_order.cancel_approval", "sales_order", id.to_string())?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}
