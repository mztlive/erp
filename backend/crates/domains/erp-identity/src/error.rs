//! 身份领域应用错误，保留原来的 IAM、认证与访问控制映射。

use application_core::{ErrorClass, ValidationIssues, impl_application_error_conversions};
use casbin::Error as CasbinError;
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

impl_repository_error!(Error, duplicate = |_| Error::ConflictError("数据已存在，请勿重复提交".into()));

impl From<MongoError> for Error {
    /// 统一经过仓储分类，保留唯一键和瞬态事务语义。
    /// # 参数
    /// error 为 MongoDB 驱动错误。
    /// # 返回
    /// 对应身份领域错误。
    /// # 错误
    /// 不返回错误。
    fn from(error: MongoError) -> Self {
        PersistenceError::from(error).into()
    }
}

impl From<CasbinError> for Error {
    /// 保留 Casbin 失败的 RBAC 分类与展示信息。
    /// # 参数
    /// error 为策略加载或权限匹配错误。
    /// # 返回
    /// RBAC 错误。
    /// # 错误
    /// 不返回错误。
    fn from(error: CasbinError) -> Self {
        Self::Rbac(error.to_string())
    }
}

impl From<ParsePermissionError> for Error {
    /// 保留迁移前权限解析失败的业务错误映射。
    /// # 参数
    /// error 为公共权限值的解析错误。
    /// # 返回
    /// 与原 erp_core 解析错误相同的身份错误。
    /// # 错误
    /// 不返回错误。
    fn from(error: ParsePermissionError) -> Self {
        Self::Logic(error.into())
    }
}

impl From<JsonError> for Error {
    /// 内部 JSON 编码失败统一归为内部错误。
    /// # 参数
    /// error 为内部数据编码错误；外部请求解析仍由协议边界处理。
    /// # 返回
    /// 保留原展示消息的内部错误。
    /// # 错误
    /// 不返回错误。
    fn from(error: JsonError) -> Self {
        Self::Internal(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{deserialize_from_document, doc};
    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};

    use super::*;

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
