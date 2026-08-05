//! 域内固定启停状态与状态机（数据模型 §6.3 `warehouse.status`、
//! `warehouse_sku_policy.status`：启用、停用）。
//!
//! 状态机：`Active ↔ Disabled` 双向迁移（对称状态机，可用
//! [`crate::common::state::assert_adjacency_closed`] 验证闭包）；
//! 数据模型第 7 章未定义本域文档状态机，第 13.3 条要求邻接矩阵固化、
//! 禁止运行时扩展。

use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;

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
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "启用",
            Self::Disabled => "停用",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    /// 判断是否处于启用状态。
    ///
    /// # 返回
    /// 处于 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

impl DocumentState for EnableStatus {
    /// 返回合法后继状态：启用 ↔ 停用 双向可迁移。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Disabled],
            Self::Disabled => &[Self::Active],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};

    /// 启用/停用双向迁移与幂等迁移合法，邻接矩阵对称闭合。
    #[test]
    fn enable_status_adjacency_is_closed() {
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);

        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Active).is_ok());
    }

    /// 状态代码与中文标签。
    #[test]
    fn enable_status_exposes_labels_and_codes() {
        assert_eq!(EnableStatus::Active.label(), "启用");
        assert_eq!(EnableStatus::Disabled.label(), "停用");
        assert_eq!(
            serde_json::to_string(&EnableStatus::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(EnableStatus::Disabled.as_str(), "disabled");
        assert!(EnableStatus::Active.is_active());
        assert!(!EnableStatus::Disabled.is_active());
    }
}
