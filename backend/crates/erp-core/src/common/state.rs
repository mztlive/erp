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
    /// 返回当前状态的全部合法后继状态。
    ///
    /// # 返回
    /// 后继状态切片（不含自身）。
    fn allowed_next(self) -> &'static [Self];
}

/// 校验一次状态迁移。
///
/// # 参数
/// * `from` - 迁移前状态
/// * `to` - 目标状态
///
/// # 返回
/// 目标在 `from.allowed_next()` 中，或 `from == to`（幂等）时返回 `Ok(())`。
///
/// # 错误
/// 目标不在后继列表中且 `from != to` 时返回 [`Error::InvalidStateTransition`]。
///
/// `S: 'static` 来自 `allowed_next` 返回 `&'static [Self]` 的要求，
/// 业务状态枚举均为 `'static`，不受影响。
pub fn ensure_transition<S: DocumentState + 'static>(from: S, to: S) -> Result<()> {
    if from == to {
        return Ok(());
    }
    if from.allowed_next().contains(&to) {
        return Ok(());
    }
    Err(Error::InvalidStateTransition {
        from: format!("{from:?}"),
        to: format!("{to:?}"),
    })
}

/// 断言状态机邻接矩阵对称闭合（测试辅助）。
///
/// # 参数
/// * `states` - 该状态机的全部状态变体（顺序不限）。需要显式传入状态列表是因为
///   Rust 泛型无法在编译期枚举枚举变体。
///
/// 断言三条不变量：
/// 1. 自反：对每个 `s`，`ensure_transition(s, s)` 恒为 `Ok`（`from == to` 不视为非法）；
/// 2. 悬空目标：`allowed_next()` 引用的每个目标都出现在 `states` 中；
/// 3. 对称闭合：对任意 `from != to`，若 `to ∈ from.allowed_next()`，则必须存在
///    `from ∈ to.allowed_next()` —— 每条迁移都成对出现，任一状态都能沿原路返回自身。
///
/// 说明：含不可逆终态（如 `CLOSED`、`VOIDED`、`REVERSED`）的状态机不满足
/// 「对称闭合」，不应调用本辅助；这类状态机应在域内对逐条边做定向断言。
///
/// # Panics
/// 任一不变量被违反时 panic，并给出具体状态名。
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

    const ALL_STATES: &[SourceSystemStatus] = &[
        SourceSystemStatus::Active,
        SourceSystemStatus::Disabled,
        SourceSystemStatus::Maintenance,
    ];

    /// 合法迁移（含幂等）全部放行。
    #[test]
    fn allowed_transitions_pass() {
        assert!(ensure_transition(SourceSystemStatus::Active, SourceSystemStatus::Active).is_ok());
        assert!(ensure_transition(SourceSystemStatus::Active, SourceSystemStatus::Disabled).is_ok());
        assert!(ensure_transition(SourceSystemStatus::Disabled, SourceSystemStatus::Maintenance).is_ok());
        assert!(ensure_transition(SourceSystemStatus::Maintenance, SourceSystemStatus::Disabled).is_ok());
    }

    /// 非法迁移返回统一错误变体，且保留 from/to 名称。
    #[test]
    fn forbidden_transitions_fail_with_unified_error() {
        let error =
            ensure_transition(SourceSystemStatus::Active, SourceSystemStatus::Maintenance).unwrap_err();
        match error {
            Error::InvalidStateTransition { from, to } => {
                assert_eq!(from, "Active");
                assert_eq!(to, "Maintenance");
            }
            other => panic!("期望 InvalidStateTransition，得到 {other:?}"),
        }
    }

    /// 邻接矩阵对称闭合：全部状态在 allowed_next 中成对出现。
    #[test]
    fn adjacency_is_symmetrically_closed() {
        assert_adjacency_closed(ALL_STATES);
    }

    /// 反例状态机（单向边）应触发闭包断言失败。
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
