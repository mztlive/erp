//! 工作流消费方端口的生产装配。

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_workflow::ports::WorkflowAccountFact;
use erp_workflow::{Error as WorkflowError, WorkItemService};
use mongodb::Database;

use crate::errors::{Error, Result};

mod approval_objects;
mod approval_scope;
mod audit;
mod authorization;
mod object_facts;
mod order_access;
mod purchase_responsibility;
mod task_scope;
mod w29_close;
pub mod work_item_authorization;

pub use audit::WorkflowAudit;
pub use authorization::WorkflowAuth;
pub use object_facts::WorkflowObjectFacts;

fn map_service(error: Error) -> WorkflowError {
    match error {
        Error::Internal(message) => WorkflowError::Internal(message),
        Error::NotFound(message) => WorkflowError::NotFound(message),
        Error::ValidationError(message) => WorkflowError::ValidationError(message),
        Error::BusinessLogicError(message) => WorkflowError::BusinessLogicError(message),
        Error::ConflictError(message) => WorkflowError::ConflictError(message),
        Error::ReceiptDuplicate(error) => WorkflowError::ReceiptDuplicate(error),
        Error::TransientTransaction(error) => WorkflowError::TransientTransaction(error),
        Error::Forbidden(message) => WorkflowError::Forbidden(message),
        Error::Unauthenticated(message) => WorkflowError::Unauthenticated(message),
        Error::Logic(error) => WorkflowError::Logic(error),
        Error::Rbac(message) => WorkflowError::Rbac(message),
        Error::OutcomeUnknown(error) => WorkflowError::OutcomeUnknown(error),
        Error::RepositoryError(error) => WorkflowError::from(error),
        Error::Coded(code) => WorkflowError::Coded(code),
    }
}

/// Convert an identity account into the workflow account fact.
pub fn account_fact(account: &erp_identity::AccountCore) -> WorkflowAccountFact {
    WorkflowAccountFact::new(account.base.id.clone(), account.kind, account.can_login())
        .with_display_name(account.name.clone())
        .with_login_account(account.secret.account().to_string())
}

/// Construct a fully wired work-item command service.
pub fn work_item_service(db: Database, rbac: SharedRbacService) -> WorkItemService<WorkflowAuth> {
    let auth = WorkflowAuth::new(db.clone(), rbac);
    let facts = Arc::new(WorkflowObjectFacts::new(db.clone()));
    let audit = Arc::new(WorkflowAudit::new(db.clone()));
    WorkItemService::with_ports(db, auth, facts, audit)
}

/// Construct workflow authorization for composition roots.
pub fn workflow_auth(db: Database, rbac: SharedRbacService) -> WorkflowAuth {
    WorkflowAuth::new(db, rbac)
}

/// Construct the workflow audit adapter.
pub fn workflow_audit(db: Database) -> Arc<WorkflowAudit> {
    Arc::new(WorkflowAudit::new(db))
}

/// Construct the work-item object-fact adapter.
pub fn workflow_object_facts(db: Database) -> Arc<WorkflowObjectFacts> {
    Arc::new(WorkflowObjectFacts::new(db))
}

/// Bind a published approval definition through composition-root ports.
///
/// Domain processes must receive `object_read` from the composition
/// root. This helper does not open a nested transaction.
pub async fn bind_published_definition_on_document_create(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    command: &erp_workflow::service::approval::binding::BindPublishedDefinitionCommand,
    actor: &application_core::AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding>> {
    let auth = WorkflowAuth::new(db.clone(), rbac.clone());
    let audit = WorkflowAudit::new(db.clone());
    erp_workflow::service::approval::binding::bind_published_definition_on_document_create(
        db,
        &auth,
        object_read,
        &audit,
        command,
        actor,
        executor,
    )
    .await
    .map_err(map_service_err)
}

fn map_service_err(error: erp_workflow::Error) -> Error {
    Error::from(error)
}

/// Attach a computed binding onto a registered business document.
pub fn attach_published_binding(
    document: &mut erp_workflow::entity::document_registry::BusinessDocument,
    binding: erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    erp_workflow::service::approval::binding::attach_published_binding(document, binding).map_err(Error::from)
}
