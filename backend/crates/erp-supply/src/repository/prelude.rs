//! Extension traits for collection repositories.

pub use super::supplier_api::{
    BusinessCapabilityConfirmationRepositoryExt, SupplierApiCapabilityRepositoryExt,
    SupplierApiConnectionRepositoryExt, SupplierConnectionCommandReceiptRepositoryExt,
    SupplierHealthCheckRunRepositoryExt,
};
pub use super::supplier_fulfillment::{
    SupplierFulfillmentItemRepositoryExt, SupplierFulfillmentOrderRepositoryExt,
    SupplierOrderActionLineRepositoryExt, SupplierOrderActionRepositoryExt,
    SupplierOrderStatusHistoryRepositoryExt, SupplierRefundAllocationRepositoryExt,
    SupplierRefundFactRepositoryExt,
};
pub use super::supplier_offering::{
    SupplierOfferingAvailabilityRepositoryExt, SupplierOfferingCommandRepositoryExt,
    SupplierOfferingRepositoryExt, SupplierOfferingRepositoryQueryExt, SupplierOfferingRevisionRepositoryExt,
    SupplierOfferingRevisionRepositoryQueryExt,
};
pub use super::supplier_settlement::{
    SupplierSettlementDifferenceEvidenceRepositoryExt, SupplierSettlementDifferenceRepositoryExt,
    SupplierSettlementItemRepositoryExt, SupplierSettlementSourceEvidenceRepositoryExt,
    SupplierSettlementStatementRepositoryExt,
};
