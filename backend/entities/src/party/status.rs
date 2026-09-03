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

/// 从有效事实集合中选择当前默认项（PROC-E04）。
///
/// 优先返回启用的默认事实，否则返回首个启用事实；全部停用或空集合返回
/// `None`。选择保持输入顺序，调用方不得在迁移时改变存量列表顺序的回退语义；
/// 令牌签发与敏感视图构造继续位于 Service。
///
/// # 参数
/// * `items` - 有效事实行切片
/// * `is_default` - 判断行是否为默认项
/// * `is_active` - 判断行是否处于启用状态
///
/// # 返回
/// 返回选中的行引用；无启用行时返回 `None`。
pub fn select_current_default<T>(
    items: &[T],
    is_default: impl Fn(&T) -> bool,
    is_active: impl Fn(&T) -> bool,
) -> Option<&T> {
    items
        .iter()
        .find(|item| is_default(item) && is_active(item))
        .or_else(|| items.iter().find(|item| is_active(item)))
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

    struct Fact {
        name: &'static str,
        is_default: bool,
        active: bool,
    }

    fn select(facts: &[Fact]) -> Option<&'static str> {
        super::select_current_default(facts, |fact| fact.is_default, |fact| fact.active).map(|fact| fact.name)
    }

    /// 默认且启用的事实优先被选中。
    #[test]
    fn select_prefers_enabled_default() {
        let facts = vec![
            Fact {
                name: "first-active",
                is_default: false,
                active: true,
            },
            Fact {
                name: "default-active",
                is_default: true,
                active: true,
            },
        ];
        assert_eq!(select(&facts), Some("default-active"));
    }

    /// 默认但停用时回退到首个启用事实。
    #[test]
    fn select_falls_back_to_first_active_when_default_disabled() {
        let facts = vec![
            Fact {
                name: "default-disabled",
                is_default: true,
                active: false,
            },
            Fact {
                name: "first-active",
                is_default: false,
                active: true,
            },
            Fact {
                name: "second-active",
                is_default: false,
                active: true,
            },
        ];
        assert_eq!(select(&facts), Some("first-active"));
    }

    /// 多个启用且无默认时选择顺序不变的首个启用事实。
    #[test]
    fn select_returns_first_active_without_default() {
        let facts = vec![
            Fact {
                name: "disabled",
                is_default: false,
                active: false,
            },
            Fact {
                name: "first-active",
                is_default: false,
                active: true,
            },
            Fact {
                name: "second-active",
                is_default: false,
                active: true,
            },
        ];
        assert_eq!(select(&facts), Some("first-active"));
    }

    /// 全部停用时返回空。
    #[test]
    fn select_returns_none_when_all_disabled() {
        let facts = vec![
            Fact {
                name: "default-disabled",
                is_default: true,
                active: false,
            },
            Fact {
                name: "other-disabled",
                is_default: false,
                active: false,
            },
        ];
        assert_eq!(select(&facts), None);
    }

    /// 空集合返回空。
    #[test]
    fn select_returns_none_for_empty_set() {
        assert_eq!(select(&[]), None);
    }
}
