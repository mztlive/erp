//! D07 从属事实行共享的启停状态（联系人/地址/税务资料/银行账户，§6.2）。
//!
//! 这些表按「有效期事实追加」维护（W03：追加有效期事实），内容变更不原地
//! 修改，启停状态是唯一允许原地切换的生命周期字段；状态机对称可逆，
//! 用 [`crate::common::state::assert_adjacency_closed`] 验证闭包。

use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::errors::Result;

/// 从属事实行的启停状态（§6.2：启用/停用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveRecordStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl EffectiveRecordStatus {
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

    /// 校验并迁移状态。
    ///
    /// 目标不在 `allowed_next()` 中且与当前状态不同时拒绝迁移（§13.3
    /// 固定邻接矩阵，禁止运行时扩展）。
    ///
    /// # 参数
    /// * `to` - 目标状态
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标状态非法时返回 [`crate::errors::Error::InvalidStateTransition`]。
    pub fn transition_to(&mut self, to: Self) -> Result<()> {
        ensure_transition(*self, to)?;
        *self = to;
        Ok(())
    }
}

impl DocumentState for EffectiveRecordStatus {
    /// 返回合法后继：启用 ⇄ 停用。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Disabled],
            Self::Disabled => &[Self::Active],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EffectiveRecordStatus;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};

    /// 状态机邻接矩阵对称闭合。
    #[test]
    fn adjacency_is_closed() {
        assert_adjacency_closed(&[EffectiveRecordStatus::Active, EffectiveRecordStatus::Disabled]);
    }

    /// 合法迁移与非法迁移均按固定矩阵判定。
    #[test]
    fn transitions_follow_fixed_matrix() {
        assert!(ensure_transition(EffectiveRecordStatus::Active, EffectiveRecordStatus::Disabled).is_ok());
        assert!(ensure_transition(EffectiveRecordStatus::Disabled, EffectiveRecordStatus::Active).is_ok());
        assert!(ensure_transition(EffectiveRecordStatus::Active, EffectiveRecordStatus::Active).is_ok());

        let mut status = EffectiveRecordStatus::Active;
        status.transition_to(EffectiveRecordStatus::Disabled).unwrap();
        assert_eq!(status, EffectiveRecordStatus::Disabled);
        assert!(
            status.transition_to(EffectiveRecordStatus::Disabled).is_ok(),
            "幂等迁移合法"
        );
        assert!(status.transition_to(EffectiveRecordStatus::Active).is_ok());
    }
}
