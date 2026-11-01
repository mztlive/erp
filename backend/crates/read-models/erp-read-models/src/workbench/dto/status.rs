//! 工作项查询状态与筛选 DTO。
//!
//! 权威类型在 `erp-workflow`；本模块再导出它们，以保持 HTTP 字段名和 serde 形状不变。

pub use erp_workflow::dto::work_item::{
    ProcessingBlockerView, ProcessingState, WORK_ITEM_TYPES, WorkItemAllowedAction, WorkItemFamily,
    WorkItemScope, WorkItemSort, family_of,
};
