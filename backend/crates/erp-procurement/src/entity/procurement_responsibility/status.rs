//! 采购责任规则启停状态与状态机（数据模型 §6.3 各表 `status`：启用、停用）。
//!
//! 状态机：`Active ↔ Disabled` 双向迁移（对称状态机，可用
//! [`erp_core::common::state::assert_adjacency_closed`] 验证闭包）；
//! 数据模型第 7 章未定义本域文档状态机，第 13.3 条要求邻接矩阵固化、
//! 禁止运行时扩展。

use serde::{Deserialize, Serialize};

use erp_core::common::state::DocumentState;

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
    use super::EnableStatus;

    /// 规则筛选与历史文档保持原小写状态代码。
    #[test]
    fn enable_status_preserves_rule_wire_codes() {
        for (status, code) in [
            (EnableStatus::Active, "active"),
            (EnableStatus::Disabled, "disabled"),
        ] {
            let value = serde_json::Value::String(code.to_string());
            assert_eq!(serde_json::to_value(status).unwrap(), value);
            assert_eq!(serde_json::from_value::<EnableStatus>(value).unwrap(), status);
            assert_eq!(status.as_str(), code);
        }
    }
}
