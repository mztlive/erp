//! 应用边界拥有的错误类型；领域错误载荷与命令恢复分类保持冻结合同。

use application_core::{ErrorClass, ValidationIssues};
use erp_audit::AuditAttemptResult;
use erp_audit::execution::AuditFailure;
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
    // 遇到应用合同错误时保留内部错误、校验错误和字段输入错误的分类，供编排入口按原语义向上返回。
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
    ($provider:ident $(, $extra:ident)*) => {
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
                    $( $provider::Error::$extra(payload) => Self::$extra(payload), )*
                }
            }
        }
    };
}

impl From<persistence_core::Error> for Error {
    // 转换持久化错误时区分版本冲突、瞬态事务和未知提交；三个历史索引保留原重复键错误，交给 HTTP 兼容分支处理，不触发回执恢复。
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

// 识别仅由 HTTP 兼容层处理的三个已归档唯一索引，避免把这些重复键归入业务命令的回执恢复范围。
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

// 向拥有唯一索引的领域取冲突提示，便于结算入口给出准确反馈；无名或未登记索引使用通用提示。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name
        .and_then(|name| {
            erp_party::known_duplicate_index_message(name)
                .or_else(|| erp_supplier::known_duplicate_index_message(name))
                .or_else(|| erp_supply::known_duplicate_index_message(name))
                .or_else(|| erp_workflow::known_duplicate_index_message(name))
        })
        .unwrap_or("数据已存在，请勿重复提交")
        .to_string()
}

impl From<validator::ValidationErrors> for Error {
    // 将校验器的字段错误封装为结构化 `InvalidInput`，让上层能指出具体输入问题。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::InvalidInput(err.into())
    }
}

impl Error {
    /// 判断写入失败后是否应查证原命令回执。仅提交结果未知、回执竞争和瞬态事务冲突返回 `true`。
    pub fn command_may_have_committed(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_) | Self::ReceiptDuplicate(_) | Self::TransientTransaction(_))
    }
    /// 将审批合同的稳定码原样封装为 `Error::Coded`，供上层按码处理，不在此重新分类。
    /// `APPROVAL_POLICY_NOT_REGISTERED` 属于内部错误，未指派给操作人属于拒绝，定义非法与驳回原因必填属于业务规则，其余属于冲突。
    pub const fn from_approval_code(code: WorkflowErrorCode) -> Self {
        Self::Coded(code)
    }

    /// 仅从 `Error::Coded` 取出稳定码，供上层按结构化错误处理；普通业务文案不反解为错误码。
    pub const fn code(&self) -> Option<WorkflowErrorCode> {
        match self {
            Self::Coded(code) => Some(*code),
            _ => None,
        }
    }
}

impl AuditFailure for Error {
    fn audit_attempt_result(&self) -> AuditAttemptResult {
        match self {
            Self::OutcomeUnknown(_) => AuditAttemptResult::Unknown,
            Self::ValidationError(_)
            | Self::BusinessLogicError(_)
            | Self::NotFound(_)
            | Self::Forbidden(_)
            | Self::Unauthenticated(_) => AuditAttemptResult::Rejected,
            Self::Coded(code) if code.class() != ErrorClass::Internal => AuditAttemptResult::Rejected,
            _ => AuditAttemptResult::Failed,
        }
    }
    fn outcome_unknown(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_))
    }
}

// 读取合同的错误载荷保持类型化。

from_domain!(erp_identity, Rbac);

from_domain!(erp_workflow, Rbac, Coded);

from_domain!(erp_party);

from_domain!(erp_supplier);

from_domain!(erp_audit);

from_domain!(erp_supply);

from_domain!(erp_finance);

from_domain!(erp_read_models, Rbac, Coded);

from_domain!(erp_supplier_orders, Rbac, Coded);

from_domain!(erp_fulfillment_execution, Rbac, Coded);

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use erp_workflow::ErrorCode;
    use mongodb::error::Error as MongoError;

    use super::{Error, WorkflowErrorCode, duplicate_index_conflict_message};

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
    fn known_party_duplicate_index_maps_to_field_message() {
        let message = duplicate_index_conflict_message(Some("uk_parties_party_no"));
        assert_eq!(message, "主体编号已存在");
    }

    #[test]
    fn fulfillment_open_task_duplicate_maps_to_refresh_message() {
        let message = duplicate_index_conflict_message(Some("uk_work_items_open_fulfillment_object"));
        assert_eq!(message, "该履约对象已存在开放任务，请刷新后重试");
    }

    #[test]
    fn customer_acceptance_open_task_duplicate_maps_to_refresh_message() {
        let message = duplicate_index_conflict_message(Some("uk_work_items_open_customer_acceptance_object"));
        assert_eq!(message, "该销售单已存在开放客户验收任务，请刷新后重试");
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
    fn approval_policy_not_registered_is_internal() {
        let error = Error::from_approval_code(ErrorCode::ApprovalPolicyNotRegistered);
        assert_eq!(error.code(), Some(ErrorCode::ApprovalPolicyNotRegistered));
        assert_eq!(error.code().expect("code").class(), ErrorClass::Internal);
    }

    #[test]
    fn contract_unprocessable_codes_are_business_logic_not_validation() {
        for code in [ErrorCode::ApprovalDefinitionInvalid, ErrorCode::ApprovalRejectReasonRequired] {
            let error = Error::from_approval_code(code);
            assert_eq!(code.class(), ErrorClass::BusinessRule, "{code} 必须是 422 语义");
            assert_eq!(error.code(), Some(code));
        }
    }

    #[test]
    fn approval_stable_codes_are_exhaustive() {
        assert_eq!(ErrorCode::ALL.len(), 22);
        assert!(ErrorCode::ALL.iter().all(|code| code.as_str().starts_with("APPROVAL_")
            || *code == ErrorCode::ResourceAuthorizationNotReady));
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
        check_domain!(erp_identity);
        check_domain!(erp_workflow);
        check_domain!(erp_party);
        check_domain!(erp_supplier);
        check_domain!(erp_audit);
        check_domain!(erp_supply);
        // 履约域已删除 `ReceiptDuplicate`（唯一冲突按 `uk_*` 索引名细化为 `ConflictError` 并携带具体文案），
        // 只覆盖其余同构变体；冲突文案路径由仓储单测覆盖。
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

    #[test]
    fn only_three_variants_allow_command_receipt_recovery() {
        let cases = [
            (Error::Internal("x".to_string()), false),
            (Error::NotFound("x".to_string()), false),
            (Error::ValidationError("x".to_string()), false),
            (Error::BusinessLogicError("x".to_string()), false),
            (Error::ConflictError("x".to_string()), false),
            (Error::ReceiptDuplicate(persistence_core::Error::DuplicateKey(MongoError::custom("x"))), true),
            (
                Error::TransientTransaction(persistence_core::Error::TransientTransactionConflict(
                    MongoError::custom("x"),
                )),
                true,
            ),
            (Error::Forbidden("x".to_string()), false),
            (Error::Unauthenticated("x".to_string()), false),
            (Error::Logic(erp_core::Error::from("x")), false),
            (Error::Rbac("x".to_string()), false),
            (
                Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(MongoError::custom("x"))),
                true,
            ),
            (Error::RepositoryError(persistence_core::Error::EntityMetadataOutOfRange("x")), false),
            (Error::Coded(WorkflowErrorCode::ApprovalTaskVersionConflict), false),
        ];
        for (error, expected) in cases {
            assert_eq!(error.command_may_have_committed(), expected, "{error:?}");
        }
    }

    #[test]
    fn read_model_bridge_preserves_typed_errors_and_workflow_code() {
        let error = Error::from(erp_read_models::Error::ReceiptDuplicate(named_duplicate("receipt-index")));
        assert!(
            matches!(&error, Error::ReceiptDuplicate(source) if source.duplicate_index_name() == Some("receipt-index"))
        );
        assert!(error.command_may_have_committed());

        let code = WorkflowErrorCode::ApprovalAlreadyStarted;
        assert!(
            matches!(Error::from(erp_read_models::Error::Coded(code)), Error::Coded(actual) if actual == code)
        );
        assert!(
            matches!(Error::from(erp_read_models::Error::Rbac("rbac".to_string())), Error::Rbac(message) if message == "rbac")
        );
    }
}
