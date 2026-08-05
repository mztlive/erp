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

    #[error("权限不足: {0}")]
    Forbidden(String),

    #[error("认证失败: {0}")]
    Unauthenticated(String),

    #[error(transparent)]
    Logic(#[from] entities::Error),

    #[error("RBAC 错误: {0}")]
    Rbac(String),

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] database::Error),

    #[error("数据库错误：{0}")]
    RepositoryError(database::Error),
}

impl From<database::Error> for Error {
    /// 将仓储错误转换为服务层错误。
    ///
    /// 唯一键、乐观锁和瞬态事务冲突保留为稳定的业务冲突语义，
    /// 其余错误保持内部仓储错误。
    fn from(error: database::Error) -> Self {
        match error {
            database::Error::DuplicateKey(_) => Self::ConflictError("数据已存在，请勿重复提交".to_string()),
            database::Error::OptimisticLockingError => {
                Self::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
            }
            database::Error::TransientTransactionConflict(_) => {
                Self::ConflictError("并发事务冲突，请重试".to_string())
            }
            error @ database::Error::CommitOutcomeUnknown(_) => Self::OutcomeUnknown(error),
            other => Self::RepositoryError(other),
        }
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 从给定值构建实例。
    ///
    /// # 参数
    /// * `err` - 错误对象
    ///
    /// # 返回
    /// 返回创建的实例。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn optimistic_locking_error_maps_to_conflict() {
        let error = Error::from(database::Error::OptimisticLockingError);

        assert!(matches!(error, Error::ConflictError(_)));
    }

    #[test]
    fn duplicate_key_error_maps_to_conflict() {
        let error = Error::from(database::Error::DuplicateKey(MongoError::custom("duplicate key")));

        assert!(matches!(error, Error::ConflictError(_)));
    }

    #[test]
    fn transient_transaction_error_maps_to_conflict() {
        let error = Error::from(database::Error::TransientTransactionConflict(MongoError::custom(
            "write conflict",
        )));

        assert!(matches!(error, Error::ConflictError(_)));
    }

    #[test]
    fn other_database_error_remains_repository_error() {
        let error = Error::from(database::Error::DatabaseError(MongoError::custom(
            "connection failed",
        )));

        assert!(matches!(error, Error::RepositoryError(_)));
    }

    #[test]
    fn unknown_commit_outcome_has_dedicated_service_semantics() {
        let error = Error::from(database::Error::CommitOutcomeUnknown(MongoError::custom(
            "unknown commit",
        )));

        assert!(matches!(&error, Error::OutcomeUnknown(_)));
        assert_eq!(
            error.to_string(),
            "操作结果暂无法确认，请查询当前状态后再决定是否重试"
        );
    }
}
