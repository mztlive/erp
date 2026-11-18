//! 工作副本编辑目的与状态（数据模型 §6.5）。
//!
//! 该文件只承载工作副本的目的、状态稳定代码、中文标签与固定状态机邻接表，
//! 不依赖表头或行实体，供 `working_copy` 聚合模块通过 `pub use` 重新导出。

use erp_core::common::state::DocumentState;
use serde::{Deserialize, Serialize};

/// 工作副本编辑目的（数据模型 §6.5：首次提交或销售变更）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkingPurpose {
    /// 首次提交。
    FirstSubmission,
    /// 销售变更。
    SalesChange,
}

impl WorkingPurpose {
    /// 为界面或响应展示提供销售工作副本编辑目的的中文名称；机器筛选和存储不使用该名称。
    pub fn label(&self) -> &'static str {
        match self {
            Self::FirstSubmission => "首次提交",
            Self::SalesChange => "销售变更",
        }
    }

    /// 将销售工作副本编辑目的转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FirstSubmission => "FIRST_SUBMISSION",
            Self::SalesChange => "SALES_CHANGE",
        }
    }
}

/// 工作副本状态（数据模型 §6.5：编辑中、已提交、已放弃、冲突）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkingCopyStatus {
    /// 编辑中。
    Editing,
    /// 已提交。
    Submitted,
    /// 已放弃。
    Abandoned,
    /// 冲突。
    Conflict,
}

impl WorkingCopyStatus {
    /// 为界面或响应展示提供销售工作副本状态的中文名称；机器筛选和存储不使用该名称。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Editing => "编辑中",
            Self::Submitted => "已提交",
            Self::Abandoned => "已放弃",
            Self::Conflict => "冲突",
        }
    }

    /// 将销售工作副本状态转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Editing => "EDITING",
            Self::Submitted => "SUBMITTED",
            Self::Abandoned => "ABANDONED",
            Self::Conflict => "CONFLICT",
        }
    }
}

impl DocumentState for WorkingCopyStatus {
    // 校验销售工作副本状态迁移时查本状态可直接到达的后继状态；无后继即不能再迁移。
    //
    // 编辑中可提交/放弃/标记冲突；冲突解决后回到编辑中；已提交/已放弃为终态
    // （驳回后以原提交复制出新的工作副本，旧副本不复用，§6.5）。
    // 结果：`Editing` 的后继为 `Submitted`、`Abandoned`、`Conflict`；`Conflict` 的后继为 `Editing`。
    // `Submitted`、`Abandoned` 返回空切片。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Editing => &[Self::Submitted, Self::Abandoned, Self::Conflict],
            Self::Conflict => &[Self::Editing],
            Self::Submitted | Self::Abandoned => &[],
        }
    }
}
