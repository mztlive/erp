//! Customer-domain application errors with the original unique-index mapping.

use application_core::ErrorClass;

/// Customer-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为客户领域错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

impl From<erp_identity::Error> for Error {
    /// 将身份域范围解析错误映射为客户领域错误。
    ///
    /// # 参数
    /// * `error` - 身份域错误
    ///
    /// # 返回
    /// 返回同构载荷的客户错误；RBAC 内部失败归入系统错误。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把身份域 Forbidden 改写成校验通过后的空集。
    fn from(error: erp_identity::Error) -> Self {
        match error {
            erp_identity::Error::Internal(payload) => Self::Internal(payload),
            erp_identity::Error::NotFound(payload) => Self::NotFound(payload),
            erp_identity::Error::ValidationError(payload) => Self::ValidationError(payload),
            erp_identity::Error::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
            erp_identity::Error::ConflictError(payload) => Self::ConflictError(payload),
            erp_identity::Error::ReceiptDuplicate(payload) => Self::ReceiptDuplicate(payload),
            erp_identity::Error::TransientTransaction(payload) => Self::TransientTransaction(payload),
            erp_identity::Error::Forbidden(payload) => Self::Forbidden(payload),
            erp_identity::Error::Unauthenticated(payload) => Self::Unauthenticated(payload),
            erp_identity::Error::Logic(payload) => Self::Logic(payload),
            erp_identity::Error::Rbac(payload) => Self::Internal(payload),
            erp_identity::Error::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
            erp_identity::Error::RepositoryError(payload) => Self::RepositoryError(payload),
        }
    }
}

/// Customer account, assignment and profile-command errors.
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
            }
            Self::BusinessLogicError(_) | Self::ValidationError(_) | Self::NotFound(_) => {
                ErrorClass::BusinessRule
            }
            Self::Forbidden(_) | Self::Unauthenticated(_) => ErrorClass::Forbidden,
            Self::OutcomeUnknown(_) => ErrorClass::Internal,
        }
    }
}

impl From<persistence_core::Error> for Error {
    /// 将仓储错误转换为客户领域错误。
    ///
    /// 唯一键、乐观锁和瞬态事务冲突保留为稳定的业务冲突语义，
    /// 其余错误保持内部仓储错误。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            error @ persistence_core::Error::DuplicateKey(_) => {
                Self::ConflictError(duplicate_key_conflict_message(&error))
            }
            persistence_core::Error::OptimisticLockingError => {
                Self::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
            }
            error @ persistence_core::Error::TransientTransactionConflict(_) => {
                Self::TransientTransaction(error)
            }
            error @ persistence_core::Error::CommitOutcomeUnknown(_) => Self::OutcomeUnknown(error),
            other => Self::RepositoryError(other),
        }
    }
}

/// 将唯一键冲突映射为面向用户的冲突提示。
fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 将客户域唯一索引名称映射为面向用户的冲突提示。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name
        .and_then(known_duplicate_index_message)
        .unwrap_or("数据已存在，请勿重复提交")
        .to_string()
}

/// 返回本域已知唯一索引的固定冲突提示。
///
/// # 参数
/// * `index_name` - 已由 persistence-core 提取的 MongoDB 索引名称
///
/// # 返回
/// 精确命中本域注册索引时返回原提示；未知名称返回 `None`，由调用边界选择通用提示。
///
/// # 约束
/// 不匹配子串或前后缀、不规范化索引名称，不拥有其他领域或 HTTP 历史索引的提示。
pub fn known_duplicate_index_message(index_name: &str) -> Option<&'static str> {
    match index_name {
        "uk_customer_accounts_party" => Some("该主体已绑定客户角色"),
        "uk_customer_accounts_customer_no" => Some("客户编号已存在"),
        _ => None,
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建客户领域错误。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;

    use super::Error;
    use application_core::ErrorClass;

    #[test]
    fn duplicate_key_error_maps_to_conflict() {
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom(
            "duplicate key",
        )));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn known_customer_duplicate_indexes_map_to_field_messages() {
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_customer_accounts_party")),
            "该主体已绑定客户角色"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_customer_accounts_customer_no")),
            "客户编号已存在"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_customer_assignments_window")),
            "数据已存在，请勿重复提交"
        );
    }

    #[test]
    fn known_duplicate_export_keeps_exact_owned_index_matrix() {
        let cases = [
            ("uk_customer_accounts_party", "该主体已绑定客户角色"),
            ("uk_customer_accounts_customer_no", "客户编号已存在"),
        ];
        for (index, expected) in cases {
            assert_eq!(super::known_duplicate_index_message(index), Some(expected));
            assert_eq!(
                super::known_duplicate_index_message(&format!("{index}_extra")),
                None
            );
            assert_eq!(super::known_duplicate_index_message(&format!(" {index}")), None);
            assert_eq!(
                super::known_duplicate_index_message(&index.to_ascii_uppercase()),
                None
            );
        }
        assert_eq!(super::known_duplicate_index_message(""), None);
        assert_eq!(super::known_duplicate_index_message("unknown_index"), None);
    }
}
