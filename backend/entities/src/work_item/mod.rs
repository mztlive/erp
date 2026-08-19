//! 域 D03 `work_item`：审批及独立人工任务的当前责任事实（页面：W01、W02）。
//!
//! `status` 只表达 `OPEN / COMPLETED / CLOSED` 生命周期；开放任务必须有个人责任人。
//! 审批推进与正式业务结果不由本域决定。

// 域 D03 与表 `work_item` 同名（domains.md 模块命名），表模块声明允许同名。
#[allow(clippy::module_inception)]
pub mod work_item;

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::WorkItemId;
pub use work_item::{
    AssignmentSource, WorkItem, WorkItemCloseData, WorkItemData, WorkItemPriority, WorkItemStatus,
    WorkItemType,
};
