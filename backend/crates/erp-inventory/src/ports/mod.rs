//! Consumer ports for authorization, warehouse/SKU/receipt facts and audit persistence.

mod audit;
mod authorization;
mod catalog;
mod fulfillment;
mod warehouse;

pub use audit::{FailClosedAuditPort, InventoryAuditPort, PreparedInventoryAudit};
pub use authorization::{
    AuthorizationPort, FailClosedAuthorizationPort, InventoryAuthorization, WarehouseScope,
};
pub use catalog::{CatalogFactsPort, FailClosedCatalogFacts, SkuFact, SkuRevisionFact};
pub use fulfillment::{FailClosedFulfillmentFacts, FulfillmentFactsPort, ReceiptNoFact};
pub use warehouse::{FailClosedWarehouseFacts, WarehouseFact, WarehouseFactsPort, WarehouseRevisionFact};
