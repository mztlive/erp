//! 按集合别名包装 [`persistence_core::Repository`]。
//!
//! 领域专用方法是泛型仓储上的扩展 trait。

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

/// 采购领域强类型独立命令回执仓储。
pub type PurchaseCommandReceiptRepository<'a, T> =
    crate::repository::command_receipt::PurchaseCommandReceiptRepository<'a, T>;
