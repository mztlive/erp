//! Fulfillment-domain application errors with the original unique-index mapping.

use application_core::ErrorClass;

/// Fulfillment-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为履约领域错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

/// Fulfillment order and revision errors.
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
            Self::ConflictError(_) | Self::TransientTransaction(_) => ErrorClass::Conflict,
            Self::BusinessLogicError(_) | Self::ValidationError(_) | Self::NotFound(_) => {
                ErrorClass::BusinessRule
            },
            Self::Forbidden(_) | Self::Unauthenticated(_) => ErrorClass::Forbidden,
            Self::OutcomeUnknown(_) => ErrorClass::Internal,
        }
    }
}

impl From<persistence_core::Error> for Error {
    /// 将仓储错误转换为履约领域错误。
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

/// 将履约域唯一索引名称映射为面向用户的冲突提示。
///
/// 未知索引回落通用文案；错误分类保持 `Conflict` 不变。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    match index_name {
        Some("uk_purchase_receipts_receipt_no") => "采购入库单号重复，请勿重复提交".to_string(),
        Some("uk_deliveries_delivery_no") => "发货单号重复，请勿重复提交".to_string(),
        Some("uk_delivery_lines_header_line") => "发货行序号重复，请勿重复提交".to_string(),
        Some("uk_electronic_deliveries_fulfillment_no") => "电子交付单号重复，请勿重复提交".to_string(),
        Some("uk_service_fulfillments_fulfillment_no") => "服务履约单号重复，请勿重复提交".to_string(),
        Some("uk_customer_acceptances_acceptance_no") => "客户验收单号重复，请勿重复提交".to_string(),
        _ => "数据已存在，请勿重复提交".to_string(),
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建履约领域错误。
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
    fn fulfillment_unique_conflicts_keep_original_messages() {
        let generic = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(generic.class(), ErrorClass::Conflict);
        assert_eq!(generic.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_purchase_receipts_receipt_no")),
            "采购入库单号重复，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_deliveries_delivery_no")),
            "发货单号重复，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_customer_acceptances_acceptance_no")),
            "客户验收单号重复，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_unknown_index")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(super::duplicate_index_conflict_message(None), "数据已存在，请勿重复提交");
    }

    #[test]
    fn optimistic_lock_maps_to_conflict_refresh_message() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }
}
