//! BPM 纯领域错误。不含 HTTP、仓储或 ERP 业务语义。
//!
//! 错误分界：本模块 `Error` 用于边界值对象构造失败
//! （`SubjectRef`/`ParticipantId`/`Timestamp`/`ProcessKind` 编码）；
//! 状态机不变式失败（字段、状态、连线、计数溢出）使用
//! `model::types::ModelError`。构造值对象失败时用前者，推进状态失败时用后者。

/// BPM 领域操作结果。
pub type Result<T> = std::result::Result<T, Error>;

/// BPM 边界类型的稳定错误。
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum Error {
    /// 流程种类稳定代码为空、超长或不在已冻结集合内。
    #[error("流程种类稳定代码无效")]
    InvalidProcessKind,

    /// 业务对象引用缺少稳定 kind/id，或超出长度上限。
    #[error("业务对象引用无效: {0}")]
    InvalidSubjectRef(&'static str),

    /// 处理人引用为空或超出长度上限。
    #[error("处理人引用无效: {0}")]
    InvalidParticipantId(&'static str),

    /// 调用方提供的 UTC 时间无法表示为时间戳。
    #[error("时间戳无效: {0}")]
    InvalidTimestamp(&'static str),
}
