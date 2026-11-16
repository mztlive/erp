//! 导入领域应用错误，并保留原唯一索引冲突映射。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use persistence_core::impl_repository_error;

/// 导入领域结果别名。
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// 旧数据导入批次、行与确认错误。
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

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] persistence_core::Error),

    #[error("数据库错误：{0}")]
    RepositoryError(persistence_core::Error),
}

impl_domain_error_class!(Error);

impl_repository_error!(
    Error,
    duplicate =
        |error| { Error::ConflictError(duplicate_index_conflict_message(error.duplicate_index_name())) }
);

/// 将导入域唯一索引名称映射为面向用户的冲突提示。
///
/// 未知索引回落通用文案；错误分类保持 `Conflict` 不变。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    match index_name {
        Some("uk_legacy_import_batches_batch_no") => "导入批次号重复，请勿重复提交".to_string(),
        Some("uk_legacy_import_confirmations_scope_trial") => {
            "该批次范围的确认已存在，请勿重复提交".to_string()
        },
        Some("uk_legacy_import_confirmations_work_item") => "该确认任务已关联确认，请勿重复提交".to_string(),
        _ => "数据已存在，请勿重复提交".to_string(),
    }
}

impl From<erp_identity::Error> for Error {
    fn from(error: erp_identity::Error) -> Self {
        match error {
            erp_identity::Error::Internal(value) | erp_identity::Error::Rbac(value) => Self::Internal(value),
            erp_identity::Error::NotFound(value) => Self::NotFound(value),
            erp_identity::Error::ValidationError(value) => Self::ValidationError(value),
            erp_identity::Error::InvalidInput(value) => Self::InvalidInput(value),
            erp_identity::Error::BusinessLogicError(value) => Self::BusinessLogicError(value),
            erp_identity::Error::ConflictError(value) => Self::ConflictError(value),
            erp_identity::Error::ReceiptDuplicate(value) => Self::ReceiptDuplicate(value),
            erp_identity::Error::TransientTransaction(value) => Self::TransientTransaction(value),
            erp_identity::Error::Forbidden(value) => Self::Forbidden(value),
            erp_identity::Error::Unauthenticated(value) => Self::Unauthenticated(value),
            erp_identity::Error::Logic(value) => Self::Logic(value),
            erp_identity::Error::OutcomeUnknown(value) => Self::OutcomeUnknown(value),
            erp_identity::Error::RepositoryError(value) => Self::from(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use erp_identity::Error as IdentityError;
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn import_unique_conflicts_keep_original_messages() {
        let generic = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(generic.class(), ErrorClass::Conflict);
        assert_eq!(generic.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_legacy_import_batches_batch_no")),
            "导入批次号重复，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_legacy_import_confirmations_scope_trial")),
            "该批次范围的确认已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_legacy_import_confirmations_work_item")),
            "该确认任务已关联确认，请勿重复提交"
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

    #[test]
    fn identity_failures_keep_error_classes_and_messages() {
        let cases = [
            (IdentityError::Forbidden("permission denied".into()), ErrorClass::Forbidden),
            (IdentityError::Unauthenticated("account inactive".into()), ErrorClass::Forbidden),
            (IdentityError::ValidationError("snapshot required".into()), ErrorClass::BusinessRule),
            (IdentityError::ConflictError("DATA_SCOPE_CHANGED".into()), ErrorClass::Conflict),
            (IdentityError::Internal("identity unavailable".into()), ErrorClass::Internal),
        ];
        for (source, class) in cases {
            let message = source.to_string();
            let converted = Error::from(source);
            assert_eq!(converted.class(), class);
            assert_eq!(converted.to_string(), message);
        }
        assert!(
            matches!(Error::from(IdentityError::Rbac("rbac unavailable".into())), Error::Internal(reason) if reason == "rbac unavailable")
        );
    }
}
