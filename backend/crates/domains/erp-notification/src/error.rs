/// 通知入口及收件箱错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("通知内容无效: {0}")]
    Invalid(&'static str),
    #[error("通知不存在")]
    NotFound,
    #[error("同一通知标识对应不同内容")]
    Conflict,
    #[error("通知渠道尚未配置")]
    ChannelUnavailable,
    #[error("通知渠道暂时不可用")]
    Retryable,
    #[error("通知渠道拒绝投递")]
    Rejected,
    #[error(transparent)]
    Repository(#[from] persistence_core::Error),
}

/// 通知域结果。
pub type Result<T> = std::result::Result<T, Error>;
