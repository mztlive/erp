//! Returns-domain application errors with the original unique-index mapping.

use application_core::ErrorClass;
use tracing::debug;

/// Returns-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为退货逆向领域错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

/// Returns order and revision errors.
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

    /// 保留给 web-api 边界映射的回款重复变体：本域 `From<persistence_core::Error>`
    /// 统一走 `ConflictError`（键上下文无法恢复资源语义），本变体不断言新的构造点。
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
    /// Stable error class used by HTTP mapping; do not parse display text.
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
    /// 将仓储错误转换为退货逆向领域错误。
    ///
    /// 唯一键、乐观锁和瞬态事务冲突保留为稳定的业务冲突语义，
    /// 其余错误保持内部仓储错误。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            error @ persistence_core::Error::DuplicateKey(_) => {
                Self::ConflictError(duplicate_key_conflict_message(&error))
            },
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

/// 将唯一键冲突映射为面向用户的冲突提示。
fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 将退货逆向域唯一索引名称映射为面向用户的冲突提示。
///
/// 退货逆向单号与明细唯一索引保持原通用冲突文案；用户文案不泄漏键细节，
/// 索引名只进 tracing 日志上下文。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    debug!(index_name = ?index_name, "唯一冲突保持通用用户文案");
    "数据已存在，请勿重复提交".to_string()
}

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建退货逆向领域错误。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn returns_unique_conflicts_keep_original_messages() {
        let generic = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(generic.class(), ErrorClass::Conflict);
        assert_eq!(generic.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_sales_return_cases_no")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_customer_refunds_no")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_payment_reversals_no")),
            "数据已存在，请勿重复提交"
        );
    }

    #[test]
    fn optimistic_lock_maps_to_conflict_refresh_message() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }

    /// 错误分类对照表：唯一冲突/乐观锁/瞬态事务恒为 Conflict，业务类恒为 BusinessRule。
    #[test]
    fn error_class_truth_table() {
        use persistence_core::Error as StoreError;

        let duplicate = Error::from(StoreError::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(duplicate.class(), ErrorClass::Conflict);
        assert!(matches!(duplicate, Error::ConflictError(_)));

        let transient =
            Error::from(StoreError::TransientTransactionConflict(MongoError::custom("transient")));
        assert_eq!(transient.class(), ErrorClass::Conflict);
        assert!(matches!(transient, Error::TransientTransaction(_)));

        for error in [
            Error::BusinessLogicError("规则".to_string()),
            Error::ValidationError("参数".to_string()),
            Error::NotFound("缺失".to_string()),
        ] {
            assert_eq!(error.class(), ErrorClass::BusinessRule);
        }
        for error in [Error::Forbidden("禁".to_string()), Error::Unauthenticated("未认证".to_string())] {
            assert_eq!(error.class(), ErrorClass::Forbidden);
        }
        assert_eq!(Error::Internal("内部".to_string()).class(), ErrorClass::Internal);
    }
}
