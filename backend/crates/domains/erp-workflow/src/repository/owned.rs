//! 按集合收窄的 [`persistence_core::Repository`] 别名。
//!
//! 领域专用方法是泛型仓储上的扩展 trait。

pub type ApprovalNotificationOutboxRepository<'a> =
    persistence_core::Repository<'a, crate::entity::approval_integration::ApprovalNotificationOutbox>;
pub type ApprovalSubjectSnapshotRepository<'a> =
    persistence_core::Repository<'a, crate::entity::approval_integration::ApprovalSubjectSnapshot>;
pub type BusinessDocumentRepository<'a> =
    persistence_core::Repository<'a, crate::entity::document_registry::BusinessDocument>;
pub type DocumentParticipantRepository<'a> =
    persistence_core::Repository<'a, crate::entity::document_registry::DocumentParticipant>;
pub type DocumentRelationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::document_registry::DocumentRelation>;
pub type FinanceResponsibilityRuleRepository<'a> =
    persistence_core::Repository<'a, crate::entity::work_item::FinanceResponsibilityRule>;
pub type WorkItemRepository<'a> = persistence_core::Repository<'a, crate::entity::work_item::WorkItem>;
pub type WorkflowActionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::document_registry::WorkflowAction>;
