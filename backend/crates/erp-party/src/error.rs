//! Party-domain application errors with the original service mapping.

use application_core::ErrorClass;

/// Party-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为主体领域错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

/// Party identity, subordinate-fact and sensitive-data errors.
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
    /// 将仓储错误转换为主体领域错误。
    ///
    /// 唯一键、乐观锁和瞬态事务冲突保留为稳定的业务冲突语义，
    /// 其余错误保持内部仓储错误。唯一键冲突优先按已知索引名给出
    /// 面向用户的字段级提示，避免一律返回笼统的「数据已存在」。
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
///
/// # 参数
/// * `error` - 已归类为 `DuplicateKey` 的仓储错误
///
/// # 返回
/// 已知索引返回字段级中文提示；无法识别时返回通用冲突提示。
fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 将唯一索引名称映射为面向用户的冲突提示。
///
/// # 参数
/// * `index_name` - MongoDB 唯一索引名称；无法识别或缺失时使用通用提示
///
/// # 返回
/// 已知索引返回字段级中文提示；未知索引返回通用冲突提示。
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
        "uk_company_names" => Some("公司全称、简称或导入别名已被其他公司使用"),
        "uk_parties_party_no" => Some("主体编号已存在"),
        "uk_parties_credit_code" => Some("统一社会信用代码已存在"),
        "uk_party_bank_accounts_bank_account_no" => Some("银行账户编号已存在"),
        "uk_party_bank_accounts_party_hmac" => Some("该主体下银行账号已存在"),
        _ => None,
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建主体领域错误。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;

    use super::{duplicate_index_conflict_message, Error};
    use application_core::ErrorClass;

    #[test]
    fn known_party_duplicate_index_maps_to_field_message() {
        let message = duplicate_index_conflict_message(Some("uk_parties_party_no"));
        assert_eq!(message, "主体编号已存在");
        assert_eq!(
            duplicate_index_conflict_message(Some("uk_parties_credit_code")),
            "统一社会信用代码已存在"
        );
        assert_eq!(
            duplicate_index_conflict_message(Some("uk_party_bank_accounts_bank_account_no")),
            "银行账户编号已存在"
        );
        assert_eq!(
            duplicate_index_conflict_message(Some("uk_party_bank_accounts_party_hmac")),
            "该主体下银行账号已存在"
        );
    }

    #[test]
    fn unknown_duplicate_index_keeps_generic_conflict_message() {
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom(
            "E11000 duplicate key error collection: erp.parties index: uk_unknown",
        )));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn known_duplicate_export_keeps_exact_owned_index_matrix() {
        let cases = [
            ("uk_parties_party_no", "主体编号已存在"),
            ("uk_parties_credit_code", "统一社会信用代码已存在"),
            ("uk_party_bank_accounts_bank_account_no", "银行账户编号已存在"),
            ("uk_party_bank_accounts_party_hmac", "该主体下银行账号已存在"),
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
