//! Work-item query status and filter DTOs.
//!
//! Canonical types live in `erp-workflow`; this module re-exports them so HTTP
//! field names and serde shapes stay unchanged.

pub use erp_workflow::dto::work_item::{
    ProcessingBlockerView, ProcessingState, WORK_ITEM_TYPES, WorkItemAllowedAction, WorkItemFamily,
    WorkItemScope, WorkItemSort, family_of,
};
