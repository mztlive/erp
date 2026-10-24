//! 身份领域应用错误，保留原来的 IAM、认证与访问控制映射。

use application_core::{ErrorClass, ValidationIssues, impl_application_error_conversions};
use persistence_core::impl_repository_error;

/// Identity-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// Identity, IAM, auth and access-control errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("系统内部错误: {0}")]
    Internal(String),

    #[error("数据不存在: {0}")]
    NotFound(String),

    #[error("参数验证失败: {0}")]
    ValidationError(String),

    /// 字段校验明细，跨边界不得压成字符串。
    #[error(transparent)]
    InvalidInput(#[from] ValidationIssues),

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

    #[error("RBAC 错误: {0}")]
    Rbac(String),

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] persistence_core::Error),

    #[error("数据库错误：{0}")]
    RepositoryError(persistence_core::Error),
}

impl Error {
    /// 返回 HTTP 映射使用的稳定错误类别；不要解析展示文案。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 冲突、业务规则、禁止或内部错误类别。`OutcomeUnknown` 归入内部错误。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn class(&self) -> ErrorClass {
        match self {
            Self::Internal(_) | Self::Logic(_) | Self::Rbac(_) | Self::RepositoryError(_) => {
                ErrorClass::Internal
            },
            Self::ConflictError(_) | Self::ReceiptDuplicate(_) | Self::TransientTransaction(_) => {
                ErrorClass::Conflict
            },
            Self::BusinessLogicError(_)
            | Self::ValidationError(_)
            | Self::InvalidInput(_)
            | Self::NotFound(_) => ErrorClass::BusinessRule,
            Self::Forbidden(_) | Self::Unauthenticated(_) => ErrorClass::Forbidden,
            Self::OutcomeUnknown(_) => ErrorClass::Internal,
        }
    }
}

impl_repository_error!(
    Error,
    duplicate = |error| { Error::ConflictError(duplicate_key_conflict_message(&error)) }
);

/// 将唯一键冲突映射为面向用户的冲突提示。
fn duplicate_key_conflict_message(_error: &persistence_core::Error) -> String {
    "数据已存在，请勿重复提交".to_string()
}
