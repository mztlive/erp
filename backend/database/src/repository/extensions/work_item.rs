//! 域 D03 `work_item`：work_item（页面：W01、W02）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D03 仓储访问器（P2 填充）。
pub trait WorkItemExt: Sized {}

impl WorkItemExt for mongodb::Database {}
