//! 域内固定启停状态与状态机（数据模型 §6.3 `warehouse.status`、
//! `warehouse_sku_policy.status`：启用、停用）。
//!
//! 状态机：`Active ↔ Disabled` 双向迁移（对称状态机，可用
//! [`erp_core::common::state::assert_adjacency_closed`] 验证闭包）；
//! 数据模型第 7 章未定义本域文档状态机，第 13.3 条要求邻接矩阵固化、
//! 禁止运行时扩展。

use erp_core::common::state::DocumentState;
use serde::{Deserialize, Serialize};

/// 启用/停用状态（数据模型 §6.3：启用、停用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnableStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl EnableStatus {
    /// 提供仓库与预警策略启停状态的中文文案，供页面和错误提示使用。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "启用",
            Self::Disabled => "停用",
        }
    }

    /// 提供启停状态的稳定代码，使持久化和按状态筛选一致。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    /// 检查状态是否为 `Active`，供履约和预警规则判断配置能否使用。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

impl DocumentState for EnableStatus {
    // 启用与停用只互为后继；不把自身列入邻接表，同状态幂等由公共迁移逻辑处理。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Disabled],
            Self::Disabled => &[Self::Active],
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::state::{assert_adjacency_closed, ensure_transition};

    use super::*;

    // 验证启用与停用双向可迁移，且同状态操作幂等，防止放开未登记的后继状态。
    #[test]
    fn enable_status_adjacency_is_closed() {
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);

        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Active).is_ok());
    }

    // 验证启停的稳定代码和中文标签，避免存储或用户展示随枚举调整而变化。
    #[test]
    fn enable_status_exposes_labels_and_codes() {
        assert_eq!(EnableStatus::Active.label(), "启用");
        assert_eq!(EnableStatus::Disabled.label(), "停用");
        assert_eq!(serde_json::to_string(&EnableStatus::Active).unwrap(), "\"active\"");
        assert_eq!(EnableStatus::Disabled.as_str(), "disabled");
        assert!(EnableStatus::Active.is_active());
        assert!(!EnableStatus::Disabled.is_active());
    }
}
