//! Warehouse domain: stable warehouse identity, revisions and SKU policies.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use application_core::PageView;
pub use dto::warehouse::{
    CreateWarehouseRequest, CreateWarehouseSkuPolicyRequest, UpdateWarehouseFulfillmentHandlersRequest,
    UpdateWarehouseRequest, UpdateWarehouseSkuPolicyRequest, WarehouseFulfillmentHandlerOptionView,
    WarehouseListParams, WarehouseRevisionListParams, WarehouseRevisionView, WarehouseSkuPolicyListParams,
    WarehouseSkuPolicyView, WarehouseView,
};
pub use entity::warehouse::warehouse_entity::{WarehouseData, WarehouseUpdate};
pub use entity::warehouse::warehouse_revision::WarehouseRevisionData;
pub use entity::warehouse::warehouse_sku_policy::{WarehouseSkuPolicyData, WarehouseSkuPolicyUpdate};
pub use entity::warehouse::{
    EnableStatus, SensitiveText, Warehouse, WarehouseFulfillmentOperation, WarehouseId, WarehouseRevision,
    WarehouseRevisionId, WarehouseSkuPolicy, WarehouseSkuPolicyId, WarehouseSkuPolicyPeriod,
};
pub use error::{Error, Result};
pub use ports::{
    AttachmentFingerprintPort, FailClosedAuditPort, FailClosedFingerprintPort, FailClosedIdentityFactPort,
    HandlerDuty, HandlerIdentityFact, IdentityFactPort, PreparedWarehouseAudit, WarehouseAuditPort,
};
pub use repository::{
    WarehouseDomainRepository, WarehouseExt, WarehouseFilter, WarehouseRepository, WarehouseRevisionFilter,
    WarehouseRevisionRepository, WarehouseRevisionRow, WarehouseRow, WarehouseSkuPolicyFilter,
    WarehouseSkuPolicyRepository, WarehouseSkuPolicyRow,
};
pub use service::warehouse::WarehouseService;
