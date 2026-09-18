//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type ProcurementResponsibilityRuleRepository<'a> = persistence_core::Repository<
    'a,
    crate::entity::procurement_responsibility::ProcurementResponsibilityRule,
>;
pub type PurchaseChangeOrderRepository<'a> =
    persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseChangeOrder>;
pub type PurchaseChangeSubmissionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseChangeSubmission>;
pub type PurchaseChangeSubmissionLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseChangeSubmissionLine>;
pub type PurchaseLineSalesAllocationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseLineSalesAllocation>;
pub type PurchaseOrderRepository<'a> =
    persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseOrder>;
pub type PurchaseOrderRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseOrderRevision>;
pub type PurchaseOrderRevisionLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseOrderRevisionLine>;
pub type PurchaseOrderSubmissionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseOrderSubmission>;
pub type PurchaseOrderSubmissionLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::purchase_order::PurchaseOrderSubmissionLine>;
