//! 公共字段基元（P0-1.4）。
//!
//! 判定表见 [`README.md`](README.md)：稳定基础资料用 `StableBase`，不可变修订用
//! `RevisionBase`，正式事实用 `FactBase`。状态机 trait 见 [`state`](state)。

pub mod fact;
pub mod revision;
pub mod source;
pub mod stable;
pub mod state;
pub mod time;
