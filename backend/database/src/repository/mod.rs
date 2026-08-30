//! MongoDB仓储实现模块
//!
//! 提供基于MongoDB的数据访问层实现

mod access_control;
mod account_core;
pub mod approval_integration;
mod audit_log;
mod base;
pub mod bpm;
mod bulk_job;
mod card_instance;
mod catalog;
mod contract;
mod cost;
mod customer;
mod customer_center_receivable;
mod customer_center_related;
mod document_registry;
pub mod extensions;
mod file_asset;
mod fulfillment;
mod integration_ops;
mod inventory;
mod legacy_import;
mod mall_after_sales;
mod mall_backfill;
mod mall_order;
mod mall_sync;
mod party;
mod payable;
mod procurement_responsibility;
mod projection;
mod publication;
mod purchase_order;
mod receivable;
mod regex_filter;
mod returns;
mod role;
mod sales_order;
mod sales_review;
mod source_registry;
mod supplier;
mod supplier_api;
mod supplier_fulfillment;
mod supplier_offering;
mod supplier_settlement;
mod warehouse;
mod work_item;
mod work_item_fulfillment_queue;

pub use audit_log::AuditLogFilter;
pub use base::{PageResult, Pagination, QueryFilter, Repository};
pub use catalog::SkuRow;
pub use customer::CustomerAccountRow;
pub use customer_center_receivable::CustomerCenterReceivableRow;
pub use customer_center_related::{
    CustomerCenterContractRow, CustomerCenterRelatedRow, CustomerCenterSalesOrderRow,
};
pub use extensions::DatabaseExt;
pub use procurement_responsibility::ProcurementResponsibilityRuleFilter;
pub use projection::{ProjectionDeliveryEscalation, ProjectionDeliveryFailure};
pub use publication::{PublicationDeliveryEscalation, PublicationDeliveryFailure};
pub use supplier::SupplierAccountRow;
pub use supplier_offering::SupplierOfferingRow;
pub use work_item::WorkItemRow;
pub use work_item_fulfillment_queue::{
    FulfillmentQueueFilter, FulfillmentQueueItemRow, FulfillmentQueueMetricRow,
    FulfillmentQueueRepositoryPage, FulfillmentQueueWarehouseRow,
};
