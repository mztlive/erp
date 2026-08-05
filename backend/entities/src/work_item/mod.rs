//! 域 D03 `work_item`：work_item（页面：W01、W02）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与必需约束见数据模型 §6.1；`work_item` 是「正式待办及处理结果」，
//! 不属稳定基础资料、不可变修订或正式事实 → 只用 `BaseModel` 持久化元数据，
//! 状态与审计字段按 §6.1 各自建模（`status` 是数据模型 §7 固定状态机之外的
//! 任务流转机，`UNCLAIMED → IN_PROGRESS → COMPLETED | CLOSED`，见本模块实现）。
//!
//! 留给 P3/P5 的跨行不变量（§6.1 必需约束）：同一业务对象、任务类型同时最多
//! 一个有效任务；领取为条件更新原子完成；正式处理校验当前领取人、对象版本与
//! 岗位分离；`workflow_action` 状态代码与目标单据状态机逐边核对。

// 域 D03 与表 `work_item` 同名（domains.md 模块命名），表模块声明允许同名。
#[allow(clippy::module_inception)]
pub mod work_item;

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::WorkItemId;
pub use work_item::{
    WorkItem, WorkItemCloseData, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
