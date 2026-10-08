//! 工作台读模型：已授权的任务列表、详情、统计和简报。

mod access;
mod action_projection;
mod approval_access;
mod material_transfer;
pub use material_transfer::authorize_material_transfer;
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
pub use inventory_settlement_brief::capture_stock_adjustment_display;
mod operational_brief;
mod owner_qualification;
mod party_names;
mod presentation;
mod purchase_review_brief;
mod query;
mod sales_order_brief;
mod stats;
mod supplier_portal_brief;
mod supply_warnings;
#[cfg(test)]
mod test_auth;

pub(crate) use dto::{
    ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemDueFilter, WorkItemFamily,
    WorkItemFamilyCountsView, WorkItemScope,
};
pub use dto::{
    WorkItemListParams, WorkItemPageView, WorkItemStatsParams, WorkItemStatsView, WorkItemView,
    work_item_destination,
};
use erp_workflow::WorkItemExt;
pub(crate) use facts::{
    ObjectKind, WorkbenchObjectFact, WorkbenchObjectFactMap, WorkbenchSubjectDisplay, object_ids,
};
pub use fulfillment_queue::{
    FulfillmentQueueGateFilter, FulfillmentQueueGateState, FulfillmentQueueItemView,
    FulfillmentQueueListParams, FulfillmentQueueMetricView, FulfillmentQueueOperationType,
    FulfillmentQueuePageView,
};
pub use query::WorkbenchReadService;

pub(crate) type WorkItemFilter = <mongodb::Database as WorkItemExt>::WorkItemFilter;

mod approval_snapshot;
pub use approval_snapshot::capture_approval_display;
mod approval_materials;
pub use approval_materials::freeze_approval_materials;

mod fulfillment_details;
