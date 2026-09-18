//! 供应链拥有仓储及窄访问器。

mod extensions;
pub mod owned;
pub mod prelude;
pub mod supplier_api;
pub mod supplier_fulfillment;
pub mod supplier_fulfillment_scope;
pub mod supplier_offering;
pub mod supplier_settlement;

pub use extensions::{SupplierApiExt, SupplierFulfillmentExt, SupplierOfferingExt, SupplierSettlementExt};
pub use prelude::{
    BusinessCapabilityConfirmationRepositoryExt, SupplierApiCapabilityRepositoryExt,
    SupplierApiConnectionRepositoryExt, SupplierConnectionCommandReceiptRepositoryExt,
    SupplierFulfillmentItemRepositoryExt, SupplierFulfillmentOrderRepositoryExt,
    SupplierHealthCheckRunRepositoryExt, SupplierOfferingAvailabilityRepositoryExt,
    SupplierOfferingCommandRepositoryExt, SupplierOfferingRepositoryExt, SupplierOfferingRepositoryQueryExt,
    SupplierOfferingRevisionRepositoryExt, SupplierOfferingRevisionRepositoryQueryExt,
    SupplierOrderActionLineRepositoryExt, SupplierOrderActionRepositoryExt,
    SupplierOrderStatusHistoryRepositoryExt, SupplierRefundAllocationRepositoryExt,
    SupplierRefundFactRepositoryExt, SupplierSettlementDifferenceEvidenceRepositoryExt,
    SupplierSettlementDifferenceRepositoryExt, SupplierSettlementItemRepositoryExt,
    SupplierSettlementSourceEvidenceRepositoryExt, SupplierSettlementStatementRepositoryExt,
};
pub use supplier_fulfillment_scope::{FulfillmentOrderReadScope, FulfillmentOrderScopeClause};
pub use supplier_offering::{
    OfferingReadScope, OfferingScopeClause, SupplierOfferingFilter, SupplierOfferingRow,
};
