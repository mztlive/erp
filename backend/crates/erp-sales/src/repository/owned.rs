//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type SalesChangeOrderRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_review::SalesChangeOrder>;
pub type SalesChangeSubmissionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_review::SalesChangeSubmission>;
pub type SalesChangeSubmissionLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_review::SalesChangeSubmissionLine>;
pub type SalesOrderRepository<'a> = persistence_core::Repository<'a, crate::entity::sales_order::SalesOrder>;
pub type SalesOrderGoodsServiceLineRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_order::SalesOrderGoodsServiceLineRevision>;
pub type SalesOrderLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_order::SalesOrderLine>;
pub type SalesOrderRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_order::SalesOrderRevision>;
pub type SalesOrderRevisionLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_order::SalesOrderRevisionLine>;
pub type SalesOrderSubmissionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_order::SalesOrderSubmission>;
pub type SalesOrderSubmissionLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_order::SalesOrderSubmissionLine>;
pub type SalesOrderVoucherLineRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_order::SalesOrderVoucherLineRevision>;
pub type SalesOrderWorkingCopyRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_order::SalesOrderWorkingCopy>;
pub type SalesOrderWorkingCopyLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_order::SalesOrderWorkingCopyLine>;
pub type SalesSelectionBookletRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_selection::SalesSelectionBooklet>;
pub type SalesSelectionDisplayItemRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_selection::SalesSelectionDisplayItem>;
pub type SalesSelectionIdempotencyRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_selection::SalesSelectionIdempotency>;
pub type SalesSelectionPoolMemberRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_selection::SalesSelectionPoolMember>;
pub type SalesSelectionPrepareTaskRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_selection::SalesSelectionPrepareTask>;
pub type SalesSelectionProposalRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_selection::SalesSelectionProposal>;
pub type SalesSelectionProposalDisplayLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_selection::SalesSelectionProposalDisplayLine>;
pub type SalesSelectionProposalSkuLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_selection::SalesSelectionProposalSkuLine>;
pub type SalesSelectionSessionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::sales_selection::SalesSelectionSession>;
