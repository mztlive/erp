//! Workflow MongoDB repositories and accessors.

pub mod approval_integration;
pub mod bpm;
pub mod document_registry;
pub mod extensions;
pub mod owned;
pub mod prelude;
pub mod work_item;

pub use approval_integration::{
    ApprovalNotificationOutboxRepositoryExt, ApprovalSubjectSnapshotRepositoryExt,
};
pub use bpm::BpmWorkflowRepository;
pub use document_registry::{
    ApprovalBindingLookup, BusinessDocumentRepositoryExt, DocumentParticipantRepositoryExt,
    DocumentRelationRepositoryExt, WorkflowActionRepositoryExt,
};
pub use extensions::{ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, WorkItemExt};
pub use owned::{
    ApprovalNotificationOutboxRepository, ApprovalSubjectSnapshotRepository, BusinessDocumentRepository,
    DocumentParticipantRepository, DocumentRelationRepository, FinanceResponsibilityRuleRepository,
    WorkItemRepository, WorkflowActionRepository,
};
pub use work_item::{
    FinanceResponsibilityRuleRepositoryExt, WorkItemFilter, WorkItemRepositoryApprovalExt,
    WorkItemRepositoryExt, WorkItemRepositoryFinanceExt, WorkItemRepositoryFulfillmentExt,
    WorkItemRepositoryIntegrationTaskBindingExt, WorkItemRow,
};
