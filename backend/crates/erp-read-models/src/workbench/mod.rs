//! Workbench read model: authorized work-item list, detail, stats and briefs.

mod access;
mod approval_list;
pub use approval_list::{ApprovalDocumentSummary, ApprovalListItem, ApprovalListPage};
pub mod authority;
mod brief;
mod change_order_brief;
mod dto;
mod facts;
mod fulfillment_operation_brief;
mod fulfillment_queue;
mod funds_document_brief;
mod inventory_settlement_brief;
mod operational_brief;
mod owner_qualification;
mod party_names;
mod presentation;
mod purchase_review_brief;
mod query;
mod sales_order_brief;
mod stats;

use erp_workflow::WorkItemExt;

pub use dto::{
    work_item_destination, WorkItemListParams, WorkItemPageView, WorkItemStatsParams, WorkItemStatsView,
    WorkItemView,
};
pub(crate) use dto::{
    ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemDueFilter, WorkItemFamily,
    WorkItemFamilyCountsView, WorkItemScope,
};
pub(crate) use facts::{
    object_ids, ObjectKind, WorkbenchObjectFact, WorkbenchObjectFactMap, WorkbenchSubjectDisplay,
};
pub use fulfillment_queue::{
    FulfillmentQueueGateFilter, FulfillmentQueueGateState, FulfillmentQueueItemView,
    FulfillmentQueueListParams, FulfillmentQueueMetricView, FulfillmentQueueOperationType,
    FulfillmentQueuePageView, FulfillmentQueueWarehouseView,
};
pub use query::WorkbenchReadService;

pub(crate) type WorkItemFilter = <mongodb::Database as WorkItemExt>::WorkItemFilter;

mod approval_snapshot;
pub use approval_snapshot::capture_approval_display;

mod fulfillment_details;
