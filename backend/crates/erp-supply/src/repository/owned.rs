//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type BusinessCapabilityConfirmationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_api::BusinessCapabilityConfirmation>;
pub type SupplierApiCapabilityRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_api::SupplierApiCapability>;
pub type SupplierApiConnectionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_api::SupplierApiConnection>;
pub type SupplierConnectionCommandReceiptRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_api::SupplierConnectionCommandReceipt>;
pub type SupplierHealthCheckRunRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_api::SupplierHealthCheckRun>;
pub type SupplierFulfillmentItemRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_fulfillment::SupplierFulfillmentItem>;
pub type SupplierFulfillmentOrderRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_fulfillment::SupplierFulfillmentOrder>;
pub type SupplierOfferingRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_offering::SupplierOffering>;
pub type SupplierOfferingAvailabilityRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_offering::SupplierOfferingAvailability>;
pub type SupplierOfferingCommandRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_offering::SupplierOfferingCommand>;
pub type SupplierOfferingRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_offering::SupplierOfferingRevision>;
pub type SupplierOrderActionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_fulfillment::SupplierOrderAction>;
pub type SupplierOrderActionLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_fulfillment::SupplierOrderActionLine>;
pub type SupplierOrderStatusHistoryRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_fulfillment::SupplierOrderStatusHistory>;
pub type SupplierRefundAllocationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_fulfillment::SupplierRefundAllocation>;
pub type SupplierRefundFactRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_fulfillment::SupplierRefundFact>;
pub type SupplierSettlementDifferenceRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_settlement::SupplierSettlementDifference>;
pub type SupplierSettlementDifferenceEvidenceRepository<'a> = persistence_core::Repository<
    'a,
    crate::entity::supplier_settlement::SupplierSettlementDifferenceEvidence,
>;
pub type SupplierSettlementItemRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_settlement::SupplierSettlementItem>;
pub type SupplierSettlementSourceEvidenceRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_settlement::SupplierSettlementSourceEvidence>;
pub type SupplierSettlementStatementRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier_settlement::SupplierSettlementStatement>;
