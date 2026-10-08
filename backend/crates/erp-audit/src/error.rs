//! 审计领域应用错误，保留原有的审计错误映射。

use application_core::ErrorClass;

/// 审计领域结果别名。
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为审计领域错误。
    ///
    /// # 参数
    /// * `error` - 应用合同错误。
    ///
    /// # 返回
    /// `Internal` 映射为 `Error::Internal`，`ValidationError` 映射为 `Error::ValidationError`，保留原文。
    ///
    /// # 错误
    /// 不返回错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

/// 审计日志与命令回执错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("系统内部错误: {0}")]
    Internal(String),

    #[error("数据不存在: {0}")]
    NotFound(String),

    #[error("参数验证失败: {0}")]
    ValidationError(String),

    #[error("业务逻辑错误: {0}")]
    BusinessLogicError(String),

    #[error("数据冲突: {0}")]
    ConflictError(String),

    #[error("数据冲突: 数据已存在，请勿重复提交")]
    ReceiptDuplicate(#[source] persistence_core::Error),

    #[error("数据冲突: 并发事务冲突，请重试")]
    TransientTransaction(#[source] persistence_core::Error),

    #[error("权限不足: {0}")]
    Forbidden(String),

    #[error("认证失败: {0}")]
    Unauthenticated(String),

    #[error(transparent)]
    Logic(#[from] erp_core::Error),

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] persistence_core::Error),

    #[error("数据库错误：{0}")]
    RepositoryError(persistence_core::Error),
}

impl Error {
    /// 返回供 HTTP 映射使用的稳定错误类别，调用方不要解析展示文案。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `Internal`、`Logic`、`RepositoryError` 与 `OutcomeUnknown` 返回 `ErrorClass::Internal`。
    /// `ConflictError`、`ReceiptDuplicate` 与 `TransientTransaction` 返回 `ErrorClass::Conflict`。
    /// `BusinessLogicError`、`ValidationError` 与 `NotFound` 返回 `ErrorClass::BusinessRule`。
    /// `Forbidden` 与 `Unauthenticated` 返回 `ErrorClass::Forbidden`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn class(&self) -> ErrorClass {
        match self {
            Self::Internal(_) | Self::Logic(_) | Self::RepositoryError(_) => ErrorClass::Internal,
            Self::ConflictError(_) | Self::ReceiptDuplicate(_) | Self::TransientTransaction(_) => {
                ErrorClass::Conflict
            },
            Self::BusinessLogicError(_) | Self::ValidationError(_) | Self::NotFound(_) => {
                ErrorClass::BusinessRule
            },
            Self::Forbidden(_) | Self::Unauthenticated(_) => ErrorClass::Forbidden,
            Self::OutcomeUnknown(_) => ErrorClass::Internal,
        }
    }
}

impl From<persistence_core::Error> for Error {
    /// 将仓储错误转换为审计领域错误。
    ///
    /// # 参数
    /// * `error` - 持久化错误。
    ///
    /// # 返回
    /// 重复键映射为 `ReceiptDuplicate`，乐观锁冲突映射为 `ConflictError`，
    /// 暂态事务冲突映射为 `TransientTransaction`，提交结果未知映射为 `OutcomeUnknown`，
    /// 其余映射为 `RepositoryError`。
    ///
    /// # 错误
    /// 不返回错误。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            error @ persistence_core::Error::DuplicateKey(_) => Self::ReceiptDuplicate(error),
            persistence_core::Error::OptimisticLockingError => {
                Self::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
            },
            error @ persistence_core::Error::TransientTransactionConflict(_) => {
                Self::TransientTransaction(error)
            },
            error @ persistence_core::Error::CommitOutcomeUnknown(_) => Self::OutcomeUnknown(error),
            other => Self::RepositoryError(other),
        }
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建审计领域错误。
    ///
    /// # 参数
    /// * `err` - `validator` 校验失败明细。
    ///
    /// # 返回
    /// 返回 `Error::ValidationError`，正文为 `err` 的展示文本。
    ///
    /// # 错误
    /// 不返回错误。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}
