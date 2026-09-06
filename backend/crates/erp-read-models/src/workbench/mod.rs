//! Workbench read model: authorized work-item list, detail, stats and briefs.

mod access;
mod brief;
mod change_order_brief;
mod dto;
mod facts;
mod fulfillment_operation_brief;
mod fulfillment_queue;
mod funds_document_brief;
mod inventory_settlement_brief;
mod party_names;
mod presentation;
mod procurement_brief;
mod purchase_review_brief;
mod query;
mod sales_order_brief;
mod stats;

use erp_workflow::WorkItemExt;

pub(crate) use dto::{
    ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemDueFilter, WorkItemFamily,
    WorkItemFamilyCountsView, WorkItemScope,
};
pub use dto::{WorkItemListParams, WorkItemPageView, WorkItemStatsParams, WorkItemStatsView, WorkItemView};
pub(crate) use facts::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, SubjectBrief};
pub use fulfillment_queue::{
    FulfillmentQueueGateFilter, FulfillmentQueueGateState, FulfillmentQueueItemView,
    FulfillmentQueueListParams, FulfillmentQueueMetricView, FulfillmentQueueOperationType,
    FulfillmentQueuePageView, FulfillmentQueueWarehouseView,
};
pub use query::WorkbenchReadService;

pub(crate) type WorkItemFilter = <mongodb::Database as WorkItemExt>::WorkItemFilter;
