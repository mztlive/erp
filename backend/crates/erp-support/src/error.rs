//! Support-domain application errors with the original service mapping.

use application_core::ErrorClass;

/// Support-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为支撑领域错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

/// Source registry, bulk job and file asset errors.
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
    /// 将仓储错误转换为支撑领域错误。
    ///
    /// 唯一键、乐观锁和瞬态事务冲突保留为稳定的业务冲突语义，
    /// 其余错误保持内部仓储错误。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            error @ persistence_core::Error::DuplicateKey(_) => {
                Self::ConflictError(duplicate_key_conflict_message(error.duplicate_index_name()))
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
///
/// 按索引名做最小映射：`request_id`/`job_no` 与外部身份键给出各自的冲突
/// 文案，未知索引回退到通用「数据已存在」文案（原先走旧 `services::Error`
/// 的未知索引分支，保持 HTTP 冲突提示不变）。`Error::class` 保持 `Conflict`。
///
/// # 参数
/// * `index` - `persistence_core::Error::duplicate_index_name` 提取的索引名；
///   `None` 表示无法识别，走通用文案。
fn duplicate_key_conflict_message(index: Option<&str>) -> String {
    let index = index.unwrap_or_default();
    if index.contains("request_id") {
        return "相同请求已提交，请勿重复提交".to_string();
    }
    if index.contains("jobs_no") {
        return "任务编号已存在，请勿重复提交".to_string();
    }
    if index.contains("external_identity") {
        return "外部身份映射已存在，请勿重复提交".to_string();
    }
    if index.contains("storage_key") {
        return "同一文件已登记，请勿重复提交".to_string();
    }
    "数据已存在，请勿重复提交".to_string()
}

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建支撑领域错误。
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
    fn external_identity_unique_conflicts_keep_conflict_class() {
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom(
            "E11000 duplicate key error collection: erp.external_identity_maps index: uk_external_identity_maps_identity",
        )));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(
            super::duplicate_key_conflict_message(Some("uk_external_identity_maps_identity")),
            "外部身份映射已存在，请勿重复提交"
        );
    }

    #[test]
    fn known_conflict_indexes_map_to_specific_messages() {
        let conflict = |index: &str| super::duplicate_key_conflict_message(Some(index));
        assert_eq!(conflict("uk_background_jobs_request_id"), "相同请求已提交，请勿重复提交");
        assert_eq!(conflict("uk_background_jobs_no"), "任务编号已存在，请勿重复提交");
        assert_eq!(conflict("uk_file_assets_storage_key"), "同一文件已登记，请勿重复提交");
        assert_eq!(super::duplicate_key_conflict_message(None), "数据已存在，请勿重复提交");
    }

    #[test]
    fn unknown_conflict_index_falls_back_to_generic_message() {
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom(
            "E11000 duplicate key error collection: erp.jobs index: uk_some_future_index",
        )));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }
}
