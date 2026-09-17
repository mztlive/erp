//! 应用边界拥有的错误类型；领域错误载荷与命令恢复分类保持冻结合同。

use erp_workflow::ErrorCode as WorkflowErrorCode;

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

    #[error("RBAC 错误: {0}")]
    Rbac(String),

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] persistence_core::Error),

    #[error("数据库错误：{0}")]
    RepositoryError(persistence_core::Error),

    #[error("{0}")]
    Coded(WorkflowErrorCode),
}

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为应用错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

// 同构提供方只做穷尽载荷移动；非同构场景 adapter 继续在各自调用点处理。
macro_rules! from_domain {
    ($provider:ident $(, $extra:ident)*) => {
        impl From<$provider::Error> for Error {
            fn from(error: $provider::Error) -> Self {
                match error {
                    $provider::Error::Internal(payload) => Self::Internal(payload),
                    $provider::Error::NotFound(payload) => Self::NotFound(payload),
                    $provider::Error::ValidationError(payload) => Self::ValidationError(payload),
                    $provider::Error::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
                    $provider::Error::ConflictError(payload) => Self::ConflictError(payload),
                    $provider::Error::ReceiptDuplicate(payload) => Self::ReceiptDuplicate(payload),
                    $provider::Error::TransientTransaction(payload) => Self::TransientTransaction(payload),
                    $provider::Error::Forbidden(payload) => Self::Forbidden(payload),
                    $provider::Error::Unauthenticated(payload) => Self::Unauthenticated(payload),
                    $provider::Error::Logic(payload) => Self::Logic(payload),
                    $provider::Error::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
                    $provider::Error::RepositoryError(payload) => Self::RepositoryError(payload),
                    $( $provider::Error::$extra(payload) => Self::$extra(payload), )*
                }
            }
        }
    };
}

from_domain!(erp_identity, Rbac);
from_domain!(erp_workflow, Rbac, Coded);
from_domain!(erp_party);
from_domain!(erp_customer);
from_domain!(erp_supplier);
from_domain!(erp_support);
from_domain!(erp_audit);
from_domain!(erp_catalog);
from_domain!(erp_warehouse);
from_domain!(erp_contract);
from_domain!(erp_inventory);
from_domain!(erp_finance);
impl From<erp_sales::Error> for Error {
    /// 将销售领域错误映射为应用错误，选品 variants 按冲突/业务规则/内部归类。
    fn from(error: erp_sales::Error) -> Self {
        match error {
            erp_sales::Error::Internal(payload) => Self::Internal(payload),
            erp_sales::Error::NotFound(payload) => Self::NotFound(payload),
            erp_sales::Error::ValidationError(payload) => Self::ValidationError(payload),
            erp_sales::Error::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
            erp_sales::Error::ConflictError(payload) => Self::ConflictError(payload),
            erp_sales::Error::SelectionConflict(payload) => Self::ConflictError(payload),
            erp_sales::Error::SelectionEnded(payload) => Self::BusinessLogicError(payload),
            erp_sales::Error::SelectionLimitExceeded(payload) => Self::BusinessLogicError(payload),
            erp_sales::Error::SelectionPrepareFailed(payload) => Self::BusinessLogicError(payload),
            erp_sales::Error::SelectionPendingCheck(payload) => Self::Internal(payload),
            erp_sales::Error::ReceiptDuplicate(payload) => Self::ReceiptDuplicate(payload),
            erp_sales::Error::TransientTransaction(payload) => Self::TransientTransaction(payload),
            erp_sales::Error::Forbidden(payload) => Self::Forbidden(payload),
            erp_sales::Error::Unauthenticated(payload) => Self::Unauthenticated(payload),
            erp_sales::Error::Logic(payload) => Self::Logic(payload),
            erp_sales::Error::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
            erp_sales::Error::RepositoryError(payload) => Self::RepositoryError(payload),
        }
    }
}
from_domain!(erp_integration);
from_domain!(erp_supply);
from_domain!(erp_returns);
impl From<erp_fulfillment::Error> for Error {
    /// 将履约领域错误映射为应用错误。
    /// 履约域已删除 `ReceiptDuplicate` 变体（唯一冲突按 `uk_*` 索引名细化为 `ConflictError`），故不用 `from_domain!`
    /// 统一宏而手写映射；分类语义不变（冲突仍归冲突）。
    fn from(error: erp_fulfillment::Error) -> Self {
        match error {
            erp_fulfillment::Error::Internal(payload) => Self::Internal(payload),
            erp_fulfillment::Error::NotFound(payload) => Self::NotFound(payload),
            erp_fulfillment::Error::ValidationError(payload) => Self::ValidationError(payload),
            erp_fulfillment::Error::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
            erp_fulfillment::Error::ConflictError(payload) => Self::ConflictError(payload),
            erp_fulfillment::Error::TransientTransaction(payload) => Self::TransientTransaction(payload),
            erp_fulfillment::Error::Forbidden(payload) => Self::Forbidden(payload),
            erp_fulfillment::Error::Unauthenticated(payload) => Self::Unauthenticated(payload),
            erp_fulfillment::Error::Logic(payload) => Self::Logic(payload),
            erp_fulfillment::Error::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
            erp_fulfillment::Error::RepositoryError(payload) => Self::RepositoryError(payload),
        }
    }
}
from_domain!(erp_procurement);
from_domain!(erp_import);

impl From<persistence_core::Error> for Error {
    /// 保留持久化分类和原错误；仅归档索引交 HTTP 的兼容分支最终形成文案。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            error @ persistence_core::Error::DuplicateKey(_) => {
                if historical_http_index(error.duplicate_index_name()) {
                    // 这三个索引无活动生产者。保留 typed DuplicateKey，不能伪装成回执重复。
                    Self::RepositoryError(error)
                } else {
                    Self::ConflictError(duplicate_key_conflict_message(&error))
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

fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 已知提示只由实际拥有域提供。无名/未知索引保持原通用冲突提示。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name
        .and_then(|name| {
            erp_party::known_duplicate_index_message(name)
                .or_else(|| erp_supplier::known_duplicate_index_message(name))
                .or_else(|| erp_supply::known_duplicate_index_message(name))
                .or_else(|| erp_contract::known_duplicate_index_message(name))
                .or_else(|| erp_procurement::known_duplicate_index_message(name))
                .or_else(|| erp_customer::known_duplicate_index_message(name))
                .or_else(|| erp_workflow::known_duplicate_index_message(name))
        })
        .unwrap_or("数据已存在，请勿重复提交")
        .to_string()
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

impl Error {
    /// 由合同冻结的审批稳定码构造应用错误。
    ///
    /// `APPROVAL_POLICY_NOT_REGISTERED` 只允许作为内部错误；资格与图校验走 422 语义，
    /// 责任不匹配走 403，其余稳定码走冲突。未接入类型不得回退旧运行时。
    ///
    /// # 参数
    /// * `code` - 合同冻结的结构化错误码
    ///
    /// # 返回
    /// 返回已带稳定码的应用错误。
    pub const fn from_approval_code(code: WorkflowErrorCode) -> Self {
        Self::Coded(code)
    }

    /// 返回结构化应用错误码。
    ///
    /// # 返回
    /// 仅结构化错误返回稳定码；普通业务文案不得被反向解析。
    pub const fn code(&self) -> Option<WorkflowErrorCode> {
        match self {
            Self::Coded(code) => Some(*code),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;

    use super::{Error, WorkflowErrorCode};

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
    fn validator_conversion_stays_plain_validation_message() {
        let mut source = validator::ValidationErrors::new();
        source.add("field", validator::ValidationError::new("invalid"));
        let expected = source.to_string();
        assert!(matches!(Error::from(source), Error::ValidationError(message) if message == expected));
    }

    #[test]
    fn domain_conversions_preserve_all_declared_payload_variants() {
        macro_rules! check_domain {
            ($provider:ident) => {
                assert!(matches!(Error::from($provider::Error::Internal("payload".to_string())), Error::Internal(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::NotFound("payload".to_string())), Error::NotFound(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::ValidationError("payload".to_string())), Error::ValidationError(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::BusinessLogicError("payload".to_string())), Error::BusinessLogicError(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::ConflictError("payload".to_string())), Error::ConflictError(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::Forbidden("payload".to_string())), Error::Forbidden(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::Unauthenticated("payload".to_string())), Error::Unauthenticated(value) if value == "payload"));
                assert!(matches!(Error::from($provider::Error::Logic(erp_core::Error::from("logic"))), Error::Logic(_)));
                let source = persistence_core::Error::DuplicateKey(MongoError::custom("receipt payload"));
                let error = Error::from($provider::Error::ReceiptDuplicate(source));
                assert!(std::error::Error::source(&error).is_some());
                assert!(matches!(error, Error::ReceiptDuplicate(persistence_core::Error::DuplicateKey(_))));
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
        check_domain!(erp_identity);
        check_domain!(erp_workflow);
        check_domain!(erp_party);
        check_domain!(erp_customer);
        check_domain!(erp_supplier);
        check_domain!(erp_support);
        check_domain!(erp_audit);
        check_domain!(erp_catalog);
        check_domain!(erp_warehouse);
        check_domain!(erp_contract);
        check_domain!(erp_inventory);
        check_domain!(erp_finance);
        check_domain!(erp_sales);
        check_domain!(erp_integration);
        check_domain!(erp_supply);
        check_domain!(erp_returns);
        check_domain!(erp_fulfillment);
        check_domain!(erp_procurement);
        check_domain!(erp_import);
        assert!(
            matches!(Error::from(erp_identity::Error::Rbac("identity".to_string())), Error::Rbac(message) if message == "identity")
        );
        assert!(
            matches!(Error::from(erp_workflow::Error::Rbac("workflow".to_string())), Error::Rbac(message) if message == "workflow")
        );
        for code in WorkflowErrorCode::ALL {
            assert!(
                matches!(Error::from(erp_workflow::Error::Coded(code)), Error::Coded(actual) if actual == code)
            );
        }
    }
}
