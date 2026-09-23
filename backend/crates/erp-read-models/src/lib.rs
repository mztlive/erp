//! Cross-domain read models: workbench, customer center and fulfillment queue.

mod errors;
mod support;

pub use errors::{Error, Result};

pub mod customer_center;
pub mod customer_quality;
pub mod fulfillment_queue;
pub mod workbench;

pub use customer_center::{
    CustomerCenterContractRow, CustomerCenterContractView, CustomerCenterReadService,
    CustomerCenterReceivableView, CustomerCenterRelatedRow, CustomerCenterRelatedView,
    CustomerCenterRepository, CustomerCenterSalesOrderRow, CustomerCenterSalesOrderView,
};
pub use fulfillment_queue::{
    FulfillmentQueueFilter, FulfillmentQueueItemRow, FulfillmentQueueMetricRow, FulfillmentQueueRepository,
    FulfillmentQueueRepositoryPage,
};
pub use workbench::{
    FulfillmentQueueGateFilter, FulfillmentQueueGateState, FulfillmentQueueItemView,
    FulfillmentQueueListParams, FulfillmentQueueMetricView, FulfillmentQueueOperationType,
    FulfillmentQueuePageView, WorkItemListParams, WorkItemPageView, WorkItemStatsParams, WorkItemStatsView,
    WorkItemView, WorkbenchReadService,
};

pub mod finance;

pub mod purchase_center;
pub mod sales_center;
pub mod supplier_center;

pub mod fulfillment_center;

pub mod returns_center;

pub mod integration_center;

pub mod catalog_center;
pub mod ports;

#[cfg(test)]
mod test_indexes;

pub mod historical_directory;
