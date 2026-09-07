//! Cross-domain read models: workbench, customer center and fulfillment queue.

mod errors {
    pub type Error = services::Error;
    pub type Result<T> = services::Result<T>;
}

pub mod customer_center;
pub mod fulfillment_queue;
pub mod workbench;

pub use customer_center::{
    CustomerCenterContractRow, CustomerCenterContractView, CustomerCenterReadService,
    CustomerCenterReceivableView, CustomerCenterRelatedRow, CustomerCenterRelatedView,
    CustomerCenterRepository, CustomerCenterSalesOrderRow, CustomerCenterSalesOrderView,
};
pub use fulfillment_queue::{
    FulfillmentQueueFilter, FulfillmentQueueItemRow, FulfillmentQueueMetricRow, FulfillmentQueueRepository,
    FulfillmentQueueRepositoryPage, FulfillmentQueueWarehouseRow,
};
pub use workbench::{
    FulfillmentQueueGateFilter, FulfillmentQueueGateState, FulfillmentQueueItemView,
    FulfillmentQueueListParams, FulfillmentQueueMetricView, FulfillmentQueueOperationType,
    FulfillmentQueuePageView, FulfillmentQueueWarehouseView, WorkItemListParams, WorkItemPageView,
    WorkItemStatsParams, WorkItemStatsView, WorkItemView, WorkbenchReadService,
};

pub mod finance;

pub mod purchase_center;
pub mod sales_center;
pub mod supplier_center;

pub mod fulfillment_center;

pub mod returns_center;
