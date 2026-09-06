//! Work-item query status and filter DTOs.
//!
//! Canonical types live in `erp-workflow`; this module re-exports them so HTTP
//! field names and serde shapes stay unchanged.

pub use erp_workflow::dto::work_item::{
    family_of, ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemFamily, WorkItemScope,
    WorkItemSort, WORK_ITEM_TYPES,
};
