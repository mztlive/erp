//! 固定状态机 trait 与测试辅助（P0-1.5 共享基元任务）。
//!
//! 数据模型第 7 章定义 8 组固定状态机，第 13 章要求「状态邻接矩阵必须固化，
//! 禁止运行时动态扩展」。各域在自己的模块内实现 [`DocumentState`]，
//! P0 只提供 trait、迁移校验函数 [`ensure_transition`] 与邻接闭包测试辅助
//! [`assert_adjacency_closed`]。

use std::fmt::Debug;

use crate::errors::{Error, Result};

/// 固定状态机要求的最小接口。
///
/// 实现者必须在 [`DocumentState::allowed_next`] 中返回全部合法后继状态；
/// `from == to` 的幂等迁移恒合法（见 [`ensure_transition`]），无需写入后继列表。
pub trait DocumentState: Sized + Copy + Eq + Debug {
    /// 由领域枚举返回当前状态的全部合法后继，供统一迁移校验查询；不要列出当前状态，同状态迁移由校验器直接放行。
    fn allowed_next(self) -> &'static [Self];
}

/// 领域修改单据状态前核对固定邻接关系。重复写入同一状态视为幂等；
/// 其他目标不在后继列表时返回 [`Error::InvalidStateTransition`]。`'static` 对应后继切片的生命周期。
pub fn ensure_transition<S: DocumentState + 'static>(from: S, to: S) -> Result<()> {
    if from == to {
        return Ok(());
    }
    if from.allowed_next().contains(&to) {
        return Ok(());
    }
    Err(Error::InvalidStateTransition { from: format!("{from:?}"), to: format!("{to:?}") })
}

/// 单测中检查要求可逆的固定状态机；请传入全部状态，泛型无法枚举其变体。
/// 逐项验证同状态幂等、目标没有遗漏、每条跨状态边都有反向边。
/// 含 `CLOSED`、`VOIDED` 或 `REVERSED` 等不可逆终态时，应在领域测试里逐条断言有向边。
///
/// # Panics
/// 发现违反规则的边或状态时 panic，并指出相关状态。
pub fn assert_adjacency_closed<S: DocumentState + 'static>(states: &[S]) {
    for &from in states {
        ensure_transition(from, from).expect("幂等迁移必须合法");

        for &to in from.allowed_next() {
            assert!(states.contains(&to), "allowed_next 引用了未枚举的状态：{to:?}");
            assert!(
                to.allowed_next().contains(&from),
                "邻接矩阵不对称：{from:?} → {to:?} 缺少反向边 {to:?} → {from:?}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 样板状态机：来源系统「启用 / 停用 / 维护中」三态。
    ///
    /// 仅用于在测试模块内演示 trait + [`ensure_transition`] + [`assert_adjacency_closed`]
    /// 的用法；D01 实施者按数据模型 6.1 在自己的模块中定义正式状态枚举。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SourceSystemStatus {
        Active,
        Disabled,
        Maintenance,
    }

    impl DocumentState for SourceSystemStatus {
        fn allowed_next(self) -> &'static [Self] {
            match self {
                Self::Active => &[Self::Disabled],
                Self::Disabled => &[Self::Active, Self::Maintenance],
                Self::Maintenance => &[Self::Disabled],
            }
        }
    }

    const ALL_STATES: &[SourceSystemStatus] =
        &[SourceSystemStatus::Active, SourceSystemStatus::Disabled, SourceSystemStatus::Maintenance];

    // 覆盖来源系统的合法状态切换与同状态重提，确保固定迁移校验不误拒。
    #[test]
    fn allowed_transitions_pass() {
        assert!(ensure_transition(SourceSystemStatus::Active, SourceSystemStatus::Active).is_ok());
        assert!(ensure_transition(SourceSystemStatus::Active, SourceSystemStatus::Disabled).is_ok());
        assert!(ensure_transition(SourceSystemStatus::Disabled, SourceSystemStatus::Maintenance).is_ok());
        assert!(ensure_transition(SourceSystemStatus::Maintenance, SourceSystemStatus::Disabled).is_ok());
    }

    // 用跳过禁用状态的请求验证拒绝迁移，并检查统一错误保留起点与目标名称。
    #[test]
    fn forbidden_transitions_fail_with_unified_error() {
        let error =
            ensure_transition(SourceSystemStatus::Active, SourceSystemStatus::Maintenance).unwrap_err();
        match error {
            Error::InvalidStateTransition { from, to } => {
                assert_eq!(from, "Active");
                assert_eq!(to, "Maintenance");
            },
            other => panic!("期望 InvalidStateTransition，得到 {other:?}"),
        }
    }

    // 确认来源系统的可逆状态机已枚举全部边，且每条跨状态边都能返回。
    #[test]
    fn adjacency_is_symmetrically_closed() {
        assert_adjacency_closed(ALL_STATES);
    }

    // 用单向迁移确认对称闭合检查不会把不可逆状态机当作可逆状态机接受。
    #[test]
    #[should_panic(expected = "邻接矩阵不对称")]
    fn one_way_edge_fails_closure() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum OneWay {
            A,
            B,
        }

        impl DocumentState for OneWay {
            fn allowed_next(self) -> &'static [Self] {
                match self {
                    Self::A => &[Self::B],
                    Self::B => &[],
                }
            }
        }

        assert_adjacency_closed(&[OneWay::A, OneWay::B]);
    }
}
