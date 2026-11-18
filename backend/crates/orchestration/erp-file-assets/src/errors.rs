//! 应用边界拥有的错误类型；领域错误载荷与命令恢复分类保持冻结合同。

use application_core::ValidationIssues;

/// 应用边界结果，不通过旧 services 或另一应用边界定义别名。
pub type Result<T> = std::result::Result<T, Error>;

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

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为应用错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
            application_core::Error::InvalidInput(message) => Self::InvalidInput(message),
        }
    }
}

// 同构提供方只做穷尽载荷移动；非同构场景 adapter 继续在各自调用点处理。
macro_rules! from_domain {
    ($provider:ident) => {
        impl From<$provider::Error> for Error {
            fn from(error: $provider::Error) -> Self {
                match error {
                    $provider::Error::Internal(payload) => Self::Internal(payload),
                    $provider::Error::NotFound(payload) => Self::NotFound(payload),
                    $provider::Error::ValidationError(payload) => Self::ValidationError(payload),
                    $provider::Error::InvalidInput(payload) => Self::InvalidInput(payload),
                    $provider::Error::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
                    $provider::Error::ConflictError(payload) => Self::ConflictError(payload),
                    $provider::Error::ReceiptDuplicate(payload) => Self::ReceiptDuplicate(payload),
                    $provider::Error::TransientTransaction(payload) => Self::TransientTransaction(payload),
                    $provider::Error::Forbidden(payload) => Self::Forbidden(payload),
                    $provider::Error::Unauthenticated(payload) => Self::Unauthenticated(payload),
                    $provider::Error::Logic(payload) => Self::Logic(payload),
                    $provider::Error::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
                    $provider::Error::RepositoryError(payload) => Self::RepositoryError(payload),
                }
            }
        }
    };
}

from_domain!(erp_support);
from_domain!(erp_audit);
impl From<persistence_core::Error> for Error {
    /// 保留持久化分类和原错误；仅归档索引交 HTTP 的兼容分支最终形成文案。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            error @ persistence_core::Error::DuplicateKey(_) => {
                if historical_http_index(error.duplicate_index_name()) {
                    // 这三个索引无活动生产者。保留 typed DuplicateKey，不能伪装成回执重复。
                    Self::RepositoryError(error)
                } else {
                    Self::ConflictError("数据已存在，请勿重复提交".to_string())
                }
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

/// 仅识别 HTTP 持有的三个历史索引；不拥有其提示文案，也不增加命令恢复范围。
fn historical_http_index(index_name: Option<&str>) -> bool {
    matches!(
        index_name,
        Some("uk_procurement_confirmation_lines_confirmation_line")
            | Some("uk_procurement_confirmation_lines_active_confirmation_line")
            | Some("uk_product_publication_revisions_publication_revision")
    )
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
        Error::InvalidInput(err.into())
    }
}

/// 按变体搬运审计错误，不重新解释持久化分类。
pub(crate) fn map_audit_to_support(error: erp_audit::Error) -> erp_support::Error {
    match error {
        erp_audit::Error::Internal(payload) => erp_support::Error::Internal(payload),
        erp_audit::Error::NotFound(payload) => erp_support::Error::NotFound(payload),
        erp_audit::Error::ValidationError(payload) => erp_support::Error::ValidationError(payload),
        erp_audit::Error::InvalidInput(payload) => erp_support::Error::InvalidInput(payload),
        erp_audit::Error::BusinessLogicError(payload) => erp_support::Error::BusinessLogicError(payload),
        erp_audit::Error::ConflictError(payload) => erp_support::Error::ConflictError(payload),
        erp_audit::Error::ReceiptDuplicate(payload) => erp_support::Error::ReceiptDuplicate(payload),
        erp_audit::Error::TransientTransaction(payload) => erp_support::Error::TransientTransaction(payload),
        erp_audit::Error::Forbidden(payload) => erp_support::Error::Forbidden(payload),
        erp_audit::Error::Unauthenticated(payload) => erp_support::Error::Unauthenticated(payload),
        erp_audit::Error::Logic(payload) => erp_support::Error::Logic(payload),
        erp_audit::Error::OutcomeUnknown(payload) => erp_support::Error::OutcomeUnknown(payload),
        erp_audit::Error::RepositoryError(payload) => erp_support::Error::RepositoryError(payload),
    }
}

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;

    use super::Error;

    fn named_duplicate(index: &str) -> persistence_core::Error {
        use mongodb::error::{ErrorKind, WriteError, WriteFailure};
        let write: WriteError = serde_json::from_value(serde_json::json!({
            "code": 11000,
            "codeName": "DuplicateKey",
            "errmsg": format!("E11000 duplicate key error index: {index} dup key: {{}}"),
            "errInfo": null,
        }))
        .expect("Mongo write error fixture");
        persistence_core::Error::from(MongoError::from(ErrorKind::Write(WriteFailure::WriteError(write))))
    }

    #[test]
    fn optimistic_locking_error_maps_to_conflict() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert!(matches!(error, Error::ConflictError(_)));
    }

    #[test]
    fn duplicate_key_error_maps_to_conflict() {
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert!(matches!(error, Error::ConflictError(_)));
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn transient_transaction_error_maps_to_conflict() {
        let error = Error::from(persistence_core::Error::TransientTransactionConflict(MongoError::custom(
            "write conflict",
        )));

        assert!(matches!(&error, Error::TransientTransaction(_)));
        assert_eq!(error.to_string(), "数据冲突: 并发事务冲突，请重试");
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn receipt_duplicate_keeps_source_and_existing_conflict_wire_message() {
        let error = Error::ReceiptDuplicate(persistence_core::Error::DuplicateKey(MongoError::custom(
            "duplicate receipt",
        )));

        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn other_database_error_remains_repository_error() {
        let error =
            Error::from(persistence_core::Error::DatabaseError(MongoError::custom("connection failed")));
        assert!(matches!(error, Error::RepositoryError(_)));
    }

    #[test]
    fn unknown_commit_outcome_has_dedicated_service_semantics() {
        let error =
            Error::from(persistence_core::Error::CommitOutcomeUnknown(MongoError::custom("unknown commit")));

        assert!(matches!(&error, Error::OutcomeUnknown(_)));
        assert_eq!(error.to_string(), "操作结果暂无法确认，请查询当前状态后再决定是否重试");
    }

    #[test]
    fn historical_indexes_keep_typed_duplicate_for_http() {
        for index in [
            "uk_procurement_confirmation_lines_confirmation_line",
            "uk_procurement_confirmation_lines_active_confirmation_line",
            "uk_product_publication_revisions_publication_revision",
        ] {
            let error = Error::from(named_duplicate(index));
            assert!(!matches!(&error, Error::ReceiptDuplicate(_)));
            let Error::RepositoryError(source) = error else {
                panic!("historical duplicate must remain typed until HTTP");
            };
            assert!(matches!(&source, persistence_core::Error::DuplicateKey(_)));
            assert_eq!(source.duplicate_index_name(), Some(index));
        }
    }

    #[test]
    fn unknown_duplicate_and_similar_historical_name_remain_ordinary_conflict() {
        for index in ["unknown_index", "uk_product_publication_revisions_publication_revision_extra"] {
            let error = Error::from(named_duplicate(index));
            assert!(matches!(&error, Error::ConflictError(message) if message == "数据已存在，请勿重复提交"));
        }
    }

    #[test]
    fn validator_conversion_preserves_field_path() {
        let mut source = validator::ValidationErrors::new();
        source.add("field", validator::ValidationError::new("invalid"));
        assert!(
            matches!(Error::from(source), Error::InvalidInput(issues) if issues.fields().contains_key("field"))
        );
    }

    #[test]
    fn domain_conversions_preserve_all_declared_payload_variants() {
        macro_rules! check_domain {
            ($provider:ident) => {
                check_domain!($provider, include_receipt_duplicate);
            };
            ($provider:ident, include_receipt_duplicate) => {
                check_domain!($provider, common_variants);
                let source = persistence_core::Error::DuplicateKey(MongoError::custom("receipt payload"));
                let error = Error::from($provider::Error::ReceiptDuplicate(source));
                assert!(std::error::Error::source(&error).is_some());
                assert!(matches!(error, Error::ReceiptDuplicate(persistence_core::Error::DuplicateKey(_))));
            };
            ($provider:ident, common_variants) => {
                let mut validation = validator::ValidationErrors::new();
                validation.add("nested.field", validator::ValidationError::new("invalid"));
                assert!(matches!(Error::from($provider::Error::InvalidInput(validation.into())), Error::InvalidInput(issues) if issues.fields().contains_key("nested.field")));
                assert!(matches!(Error::from($provider::Error::Internal("payload".to_string())), Error::Internal(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::NotFound("payload".to_string())), Error::NotFound(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::ValidationError("payload".to_string())), Error::ValidationError(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::BusinessLogicError("payload".to_string())), Error::BusinessLogicError(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::ConflictError("payload".to_string())), Error::ConflictError(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::Forbidden("payload".to_string())), Error::Forbidden(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::Unauthenticated("payload".to_string())), Error::Unauthenticated(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::Logic(erp_core::Error::from("logic"))), Error::Logic(_)));
                let source = persistence_core::Error::TransientTransactionConflict(MongoError::custom("transient payload"));
                let error = Error::from($provider::Error::TransientTransaction(source));
                assert!(std::error::Error::source(&error).is_some());
                assert!(matches!(error, Error::TransientTransaction(persistence_core::Error::TransientTransactionConflict(_))));
                let source = persistence_core::Error::CommitOutcomeUnknown(MongoError::custom("unknown payload"));
                let error = Error::from($provider::Error::OutcomeUnknown(source));
                assert!(std::error::Error::source(&error).is_some());
                assert!(matches!(error, Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(_))));
                let source = persistence_core::Error::EntityMetadataOutOfRange("field-marker");
                let error = Error::from($provider::Error::RepositoryError(source));
                assert!(std::error::Error::source(&error).is_none());
                assert!(matches!(error, Error::RepositoryError(persistence_core::Error::EntityMetadataOutOfRange("field-marker"))));
            };
        }
        check_domain!(erp_support);
        check_domain!(erp_audit);
        // 履约域已删除 `ReceiptDuplicate`（唯一冲突按 `uk_*` 索引名细化为 `ConflictError` 并携带具体文案），
        // 只覆盖其余同构变体；冲突文案路径由仓储单测覆盖。
    }
}
