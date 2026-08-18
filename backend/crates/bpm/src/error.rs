//! BPM 纯领域错误。不含 HTTP、仓储或 ERP 业务语义。

/// BPM 领域操作结果。
pub type Result<T> = std::result::Result<T, Error>;

/// BPM 边界类型与尚未接线入口的稳定错误。
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

    /// 目标引擎或编排入口尚未接线，必须失败关闭。
    #[error("BPM 目标能力尚未接线，已按安全策略拒绝")]
    NotWired,
}
