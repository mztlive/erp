//! 身份领域应用错误，保留原来的 IAM、认证与访问控制映射。

use application_core::{ErrorClass, ValidationIssues, impl_application_error_conversions};
use erp_permissions::ParsePermissionError;
use mongodb::error::Error as MongoError;
use persistence_core::{Error as PersistenceError, impl_repository_error};
use serde_json::Error as JsonError;

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
    /// 提供 HTTP 层可直接使用的稳定错误分类，避免解析展示文案。结果不确定的事务提交按内部错误处理。
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

impl_repository_error!(Error, duplicate = |_| Error::ConflictError("数据已存在，请勿重复提交".into()));

impl From<MongoError> for Error {
    // 将驱动错误交给统一仓储分类，保留唯一键和瞬态事务错误的业务映射。
    fn from(error: MongoError) -> Self {
        PersistenceError::from(error).into()
    }
}

impl From<ParsePermissionError> for Error {
    // 将公共权限值解析失败转换为身份业务错误，保留底层错误类型。
    fn from(error: ParsePermissionError) -> Self {
        Self::Logic(error.into())
    }
}

impl From<JsonError> for Error {
    // 将身份服务内部的 JSON 编码故障归入内部错误；外部请求解析由协议层处理。
    fn from(error: JsonError) -> Self {
        Self::Internal(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{deserialize_from_document, doc};
    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};

    use super::*;

    // 验证 MongoDB 唯一键异常保留冲突分类，普通驱动故障保留仓储错误分类。
    #[test]
    fn driver_errors_keep_repository_classification() {
        let write_error: WriteError = deserialize_from_document(doc! {
            "code": 11000, "errmsg": "duplicate", "errInfo": null,
        })
        .unwrap();
        let duplicate: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
        let error = Error::from(duplicate);
        assert!(matches!(&error, Error::ConflictError(message) if message == "数据已存在，请勿重复提交"));
        assert_eq!(error.class(), ErrorClass::Conflict);

        let failure = Error::from(MongoError::custom("unavailable"));
        assert!(matches!(failure, Error::RepositoryError(_)));
        assert_eq!(failure.class(), ErrorClass::Internal);
    }

    // 验证瞬态事务冲突和提交结果不明分别保留错误分类，供调用方决定重试或先核查结果。
    #[test]
    fn transaction_failures_preserve_retry_and_unknown_outcome() {
        let transient =
            Error::from(persistence_core::Error::TransientTransactionConflict(MongoError::custom("retry")));
        assert!(matches!(transient, Error::TransientTransaction(_)));
        assert_eq!(transient.class(), ErrorClass::Conflict);

        let unknown =
            Error::from(persistence_core::Error::CommitOutcomeUnknown(MongoError::custom("unknown")));
        assert!(matches!(unknown, Error::OutcomeUnknown(_)));
        assert_eq!(unknown.class(), ErrorClass::Internal);
    }
}
