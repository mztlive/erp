//! 财务领域应用错误，并保留唯一索引冲突的既有映射。

use application_core::ErrorClass;

/// 财务领域结果别名。
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为财务领域错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

/// 财务账户、发票、付款与成本错误。
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
    /// 返回供 HTTP 映射使用的稳定错误类别；不要解析展示文案。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `Internal`、`Logic`、`RepositoryError` 与 `OutcomeUnknown` 为 `ErrorClass::Internal`；
    /// 冲突类为 `ErrorClass::Conflict`；业务、校验与不存在为 `ErrorClass::BusinessRule`；
    /// 权限与认证失败为 `ErrorClass::Forbidden`。
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
    /// 将仓储错误转换为财务领域错误。
    ///
    /// 唯一键、乐观锁和瞬态事务冲突保留为稳定的业务冲突语义，
    /// 其余错误保持内部仓储错误。
    ///
    /// # 参数
    /// * `error` - 仓储或事务执行返回的错误
    ///
    /// # 返回
    /// 返回保留原错误分类和提示文案的财务领域错误。
    ///
    /// # 错误
    /// 无；本转换不执行数据库操作。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            persistence_core::Error::DuplicateKey(_) => {
                Self::ConflictError("数据已存在，请勿重复提交".to_string())
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

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建财务领域错误。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::bson::{deserialize_from_document, doc};
    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};

    use super::Error;

    #[test]
    fn finance_unique_conflicts_keep_original_messages() {
        for index in [
            "uk_customer_receipts_no",
            "uk_receivable_accounts_sales_order",
            "uk_invoices_coded",
            "uk_unknown_index",
        ] {
            let write_error: WriteError = deserialize_from_document(doc! {
                "code": 11000,
                "codeName": "DuplicateKey",
                "errmsg": format!("E11000 duplicate key collection: erp.finance index: {index} dup key: {{ id: 1 }}"),
                "errInfo": null,
            })
            .expect("唯一键错误样本应可反序列化");
            let mongo_error: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
            let repository_error = persistence_core::Error::from(mongo_error);
            assert_eq!(repository_error.duplicate_index_name(), Some(index));
            let error = Error::from(repository_error);
            assert_eq!(error.class(), ErrorClass::Conflict);
            assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        }
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn optimistic_lock_maps_to_conflict_refresh_message() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }
}
