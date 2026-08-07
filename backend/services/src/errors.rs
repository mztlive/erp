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
    /// 其余错误保持内部仓储错误。唯一键冲突优先按已知索引名给出
    /// 面向用户的字段级提示，避免一律返回笼统的「数据已存在」。
    fn from(error: database::Error) -> Self {
        match error {
            error @ database::Error::DuplicateKey(_) => {
                Self::ConflictError(duplicate_key_conflict_message(&error))
            }
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

/// 将唯一键冲突映射为面向用户的冲突提示。
///
/// # 参数
/// * `error` - 已归类为 `DuplicateKey` 的仓储错误
///
/// # 返回
/// 已知索引返回字段级中文提示；无法识别时返回通用冲突提示。
fn duplicate_key_conflict_message(error: &database::Error) -> String {
    match error.duplicate_index_name() {
        Some("uk_parties_party_no") => "主体编号已存在".to_string(),
        Some("uk_parties_credit_code") => "统一社会信用代码已存在".to_string(),
        Some("uk_party_bank_accounts_bank_account_no") => "银行账户编号已存在".to_string(),
        Some("uk_party_bank_accounts_party_hmac") => "该主体下银行账号已存在".to_string(),
        Some("uk_supplier_accounts_party") => "该主体已绑定供应商角色".to_string(),
        Some("uk_supplier_accounts_supplier_no") => "供应商编号已存在".to_string(),
        Some("uk_customer_accounts_party") => "该主体已绑定客户角色".to_string(),
        Some("uk_customer_accounts_customer_no") => "客户编号已存在".to_string(),
        _ => "数据已存在，请勿重复提交".to_string(),
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
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn known_party_duplicate_index_maps_to_field_message() {
        use mongodb::{
            bson::{doc, from_document},
            error::{ErrorKind, WriteError, WriteFailure},
        };

        let write_error: WriteError = from_document(doc! {
            "code": 11000,
            "codeName": "DuplicateKey",
            "errmsg": "E11000 duplicate key error collection: erp.parties index: uk_parties_party_no dup key: { party_no: \"PTY-1\" }",
            "errInfo": null,
        })
        .expect("write error fixture should deserialize");
        let mongo_error: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
        let error = Error::from(database::Error::DuplicateKey(mongo_error));

        assert_eq!(error.to_string(), "数据冲突: 主体编号已存在");
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
