//! Workflow domain: approval integration, document registry and work-item commands.

#![allow(clippy::too_many_arguments)]

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use dto::approval::{
    ApprovalCancelBlockedCommand, ApprovalCancelCommand, ApprovalDecisionCommand,
    ApprovalRecoveryAuthorization, ApprovalResumeCommand, ApprovalStartCommand,
};
pub use entity::approval_integration::{
    ApprovalNotificationOutbox, ApprovalSubjectSnapshot, SalesBusinessKind, document_type_from_subject_kind,
    document_type_of, process_kind_of, subject_ref_for,
};
pub use entity::document_registry::{
    BusinessDocument, DocumentParticipant, DocumentRelation, DocumentType, WorkflowAction,
};
pub use entity::work_item::{
    AvailableWorkItemAccount, FinanceResponsibilityOperation, FinanceResponsibilityRule, WorkItem,
    WorkItemData, WorkItemStatus, WorkItemType,
};
pub use error::{Error, ErrorCode, Result, known_duplicate_index_message};
pub use ports::{
    ApprovalObjectReadPort, ApprovalUpgradeSubjectFacts, FailClosedObjectReadPort,
    FailClosedWorkflowAuthorizationPort, ObjectFactPort, UpgradeSubjectPort, WorkflowAuthorizationPort,
};
pub use repository::{
    ApprovalBindingLookup, ApprovalIntegrationExt, BpmExt, BpmWorkflowRepository, DocumentRegistryExt,
    WorkItemExt, WorkItemFilter, WorkItemRepository, WorkItemRow,
};
pub use service::approval::action::{
    ApprovalActionContext, ApprovalActionFuture, ApprovalDomainActionPort, FailClosedApprovalActionPort,
};
pub use service::approval::definition::ApprovalDefinitionService;
pub use service::approval::execution::{ApprovalNotificationOutboxPort, ApprovalRuntimeService};
pub use service::approval::policy::ApprovalDomainAction;
pub use service::document_registry::DocumentRegistryService;
pub use service::work_item::WorkItemService;
