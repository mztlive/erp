//! Extension traits for collection repositories.

pub use super::approval_integration::{
    ApprovalNotificationOutboxRepositoryExt, ApprovalSubjectSnapshotRepositoryExt,
};
pub use super::document_registry::{
    BusinessDocumentRepositoryExt, DocumentParticipantRepositoryExt, DocumentRelationRepositoryExt,
    WorkflowActionRepositoryExt,
};
pub use super::work_item::{
    FinanceResponsibilityRuleRepositoryExt, WorkItemRepositoryApprovalExt, WorkItemRepositoryExt,
    WorkItemRepositoryFinanceExt, WorkItemRepositoryFulfillmentExt,
    WorkItemRepositoryIntegrationTaskBindingExt,
};
