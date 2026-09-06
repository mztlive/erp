//! Owned workflow repositories composed from persistence-core.

mod approval_notification_outbox;
mod approval_subject_snapshot;
mod business_document;
mod document_participant;
mod document_relation;
mod finance_responsibility_rule;
mod work_item;
mod workflow_action;

pub use approval_notification_outbox::ApprovalNotificationOutboxRepository;
pub use approval_subject_snapshot::ApprovalSubjectSnapshotRepository;
pub use business_document::BusinessDocumentRepository;
pub use document_participant::DocumentParticipantRepository;
pub use document_relation::DocumentRelationRepository;
pub use finance_responsibility_rule::FinanceResponsibilityRuleRepository;
pub use work_item::WorkItemRepository;
pub use workflow_action::WorkflowActionRepository;
