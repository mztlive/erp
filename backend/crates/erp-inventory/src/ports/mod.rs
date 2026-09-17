//! Consumer ports for authorization, warehouse/SKU/receipt facts and audit persistence.

mod audit;
mod authorization;
mod catalog;
mod fulfillment;
mod people;
mod warehouse;

pub use audit::{FailClosedAuditPort, InventoryAuditPort, PreparedInventoryAudit};
pub use authorization::{
    AuthorizationPort, FailClosedAuthorizationPort, InventoryAuthorization, InventoryScopeMeta,
    WarehouseScope,
};
pub use catalog::{CatalogFactsPort, FailClosedCatalogFacts, SkuFact, SkuRevisionFact};
pub use fulfillment::{FailClosedFulfillmentFacts, FulfillmentFactsPort, ReceiptNoFact};
pub use people::{
    AdjustmentPeopleFact, AdjustmentPeopleFactsPort, FailClosedAdjustmentPeopleFactsPort,
    applicant_object_ids, intersect_object_ids, latest_snapshot_submitters, merge_adjustment_people,
};
pub use warehouse::{FailClosedWarehouseFacts, WarehouseFact, WarehouseFactsPort, WarehouseRevisionFact};
