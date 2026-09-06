use application_core::ErrorClass;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为服务错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

impl From<erp_identity::Error> for Error {
    /// 将身份领域错误映射为服务错误。
    fn from(error: erp_identity::Error) -> Self {
        match error {
            erp_identity::Error::Internal(message) => Self::Internal(message),
            erp_identity::Error::NotFound(message) => Self::NotFound(message),
            erp_identity::Error::ValidationError(message) => Self::ValidationError(message),
            erp_identity::Error::BusinessLogicError(message) => Self::BusinessLogicError(message),
            erp_identity::Error::ConflictError(message) => Self::ConflictError(message),
            erp_identity::Error::ReceiptDuplicate(error) => Self::ReceiptDuplicate(error),
            erp_identity::Error::TransientTransaction(error) => Self::TransientTransaction(error),
            erp_identity::Error::Forbidden(message) => Self::Forbidden(message),
            erp_identity::Error::Unauthenticated(message) => Self::Unauthenticated(message),
            erp_identity::Error::Logic(error) => Self::Logic(error),
            erp_identity::Error::Rbac(message) => Self::Rbac(message),
            erp_identity::Error::OutcomeUnknown(error) => Self::OutcomeUnknown(error),
            erp_identity::Error::RepositoryError(error) => Self::RepositoryError(error),
        }
    }
}

impl From<erp_workflow::Error> for Error {
    /// 将工作流领域错误映射为服务错误。
    fn from(error: erp_workflow::Error) -> Self {
        match error {
            erp_workflow::Error::Internal(message) => Self::Internal(message),
            erp_workflow::Error::NotFound(message) => Self::NotFound(message),
            erp_workflow::Error::ValidationError(message) => Self::ValidationError(message),
            erp_workflow::Error::BusinessLogicError(message) => Self::BusinessLogicError(message),
            erp_workflow::Error::ConflictError(message) => Self::ConflictError(message),
            erp_workflow::Error::ReceiptDuplicate(error) => Self::ReceiptDuplicate(error),
            erp_workflow::Error::TransientTransaction(error) => Self::TransientTransaction(error),
            erp_workflow::Error::Forbidden(message) => Self::Forbidden(message),
            erp_workflow::Error::Unauthenticated(message) => Self::Unauthenticated(message),
            erp_workflow::Error::Logic(error) => Self::Logic(error),
            erp_workflow::Error::Rbac(message) => Self::Rbac(message),
            erp_workflow::Error::OutcomeUnknown(error) => Self::OutcomeUnknown(error),
            erp_workflow::Error::RepositoryError(error) => Self::RepositoryError(error),
            erp_workflow::Error::Coded(code) => Self::Coded(ErrorCode::from(code)),
        }
    }
}

impl From<erp_party::Error> for Error {
    /// 将主体领域错误映射为服务错误。
    fn from(error: erp_party::Error) -> Self {
        match error {
            erp_party::Error::Internal(message) => Self::Internal(message),
            erp_party::Error::NotFound(message) => Self::NotFound(message),
            erp_party::Error::ValidationError(message) => Self::ValidationError(message),
            erp_party::Error::BusinessLogicError(message) => Self::BusinessLogicError(message),
            erp_party::Error::ConflictError(message) => Self::ConflictError(message),
            erp_party::Error::ReceiptDuplicate(error) => Self::ReceiptDuplicate(error),
            erp_party::Error::TransientTransaction(error) => Self::TransientTransaction(error),
            erp_party::Error::Forbidden(message) => Self::Forbidden(message),
            erp_party::Error::Unauthenticated(message) => Self::Unauthenticated(message),
            erp_party::Error::Logic(error) => Self::Logic(error),
            erp_party::Error::OutcomeUnknown(error) => Self::OutcomeUnknown(error),
            erp_party::Error::RepositoryError(error) => Self::RepositoryError(error),
        }
    }
}

impl From<erp_customer::Error> for Error {
    /// 将客户领域错误映射为服务错误。
    fn from(error: erp_customer::Error) -> Self {
        match error {
            erp_customer::Error::Internal(message) => Self::Internal(message),
            erp_customer::Error::NotFound(message) => Self::NotFound(message),
            erp_customer::Error::ValidationError(message) => Self::ValidationError(message),
            erp_customer::Error::BusinessLogicError(message) => Self::BusinessLogicError(message),
            erp_customer::Error::ConflictError(message) => Self::ConflictError(message),
            erp_customer::Error::ReceiptDuplicate(error) => Self::ReceiptDuplicate(error),
            erp_customer::Error::TransientTransaction(error) => Self::TransientTransaction(error),
            erp_customer::Error::Forbidden(message) => Self::Forbidden(message),
            erp_customer::Error::Unauthenticated(message) => Self::Unauthenticated(message),
            erp_customer::Error::Logic(error) => Self::Logic(error),
            erp_customer::Error::OutcomeUnknown(error) => Self::OutcomeUnknown(error),
            erp_customer::Error::RepositoryError(error) => Self::RepositoryError(error),
        }
    }
}

impl From<erp_supplier::Error> for Error {
    /// 将供应商领域错误映射为服务错误。
    fn from(error: erp_supplier::Error) -> Self {
        match error {
            erp_supplier::Error::Internal(message) => Self::Internal(message),
            erp_supplier::Error::NotFound(message) => Self::NotFound(message),
            erp_supplier::Error::ValidationError(message) => Self::ValidationError(message),
            erp_supplier::Error::BusinessLogicError(message) => Self::BusinessLogicError(message),
            erp_supplier::Error::ConflictError(message) => Self::ConflictError(message),
            erp_supplier::Error::ReceiptDuplicate(error) => Self::ReceiptDuplicate(error),
            erp_supplier::Error::TransientTransaction(error) => Self::TransientTransaction(error),
            erp_supplier::Error::Forbidden(message) => Self::Forbidden(message),
            erp_supplier::Error::Unauthenticated(message) => Self::Unauthenticated(message),
            erp_supplier::Error::Logic(error) => Self::Logic(error),
            erp_supplier::Error::OutcomeUnknown(error) => Self::OutcomeUnknown(error),
            erp_supplier::Error::RepositoryError(error) => Self::RepositoryError(error),
        }
    }
}

impl From<erp_support::Error> for Error {
    /// 将支撑领域错误映射为服务错误。
    fn from(error: erp_support::Error) -> Self {
        match error {
            erp_support::Error::Internal(message) => Self::Internal(message),
            erp_support::Error::NotFound(message) => Self::NotFound(message),
            erp_support::Error::ValidationError(message) => Self::ValidationError(message),
            erp_support::Error::BusinessLogicError(message) => Self::BusinessLogicError(message),
            erp_support::Error::ConflictError(message) => Self::ConflictError(message),
            erp_support::Error::ReceiptDuplicate(error) => Self::ReceiptDuplicate(error),
            erp_support::Error::TransientTransaction(error) => Self::TransientTransaction(error),
            erp_support::Error::Forbidden(message) => Self::Forbidden(message),
            erp_support::Error::Unauthenticated(message) => Self::Unauthenticated(message),
            erp_support::Error::Logic(error) => Self::Logic(error),
            erp_support::Error::OutcomeUnknown(error) => Self::OutcomeUnknown(error),
            erp_support::Error::RepositoryError(error) => Self::RepositoryError(error),
        }
    }
}

impl From<erp_audit::Error> for Error {
    /// 将审计领域错误映射为服务错误。
    fn from(error: erp_audit::Error) -> Self {
        match error {
            erp_audit::Error::Internal(message) => Self::Internal(message),
            erp_audit::Error::NotFound(message) => Self::NotFound(message),
            erp_audit::Error::ValidationError(message) => Self::ValidationError(message),
            erp_audit::Error::BusinessLogicError(message) => Self::BusinessLogicError(message),
            erp_audit::Error::ConflictError(message) => Self::ConflictError(message),
            erp_audit::Error::ReceiptDuplicate(error) => Self::ReceiptDuplicate(error),
            erp_audit::Error::TransientTransaction(error) => Self::TransientTransaction(error),
            erp_audit::Error::Forbidden(message) => Self::Forbidden(message),
            erp_audit::Error::Unauthenticated(message) => Self::Unauthenticated(message),
            erp_audit::Error::Logic(error) => Self::Logic(error),
            erp_audit::Error::OutcomeUnknown(error) => Self::OutcomeUnknown(error),
            erp_audit::Error::RepositoryError(error) => Self::RepositoryError(error),
        }
    }
}

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
    Coded(ErrorCode),
}

impl From<persistence_core::Error> for Error {
    /// 将仓储错误转换为服务层错误。
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
///
/// # 错误
/// 无。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    match index_name {
        Some("uk_parties_party_no") => "主体编号已存在".to_string(),
        Some("uk_parties_credit_code") => "统一社会信用代码已存在".to_string(),
        Some("uk_party_bank_accounts_bank_account_no") => "银行账户编号已存在".to_string(),
        Some("uk_party_bank_accounts_party_hmac") => "该主体下银行账号已存在".to_string(),
        Some("uk_supplier_accounts_party") => "该主体已绑定供应商角色".to_string(),
        Some("uk_supplier_accounts_supplier_no") => "供应商编号已存在".to_string(),
        Some("uk_supplier_offerings_supplier_sku") => "该供应商 SKU 已登记供给".to_string(),
        Some("uk_contracts_contract_no") => "合同编号已存在".to_string(),
        Some("uk_purchase_orders_creation_basis") => "该采购创建依据已生成采购单".to_string(),
        Some("uk_customer_accounts_party") => "该主体已绑定客户角色".to_string(),
        Some("uk_customer_accounts_customer_no") => "客户编号已存在".to_string(),
        Some("uk_procurement_confirmation_lines_confirmation_line")
        | Some("uk_procurement_confirmation_lines_active_confirmation_line") => {
            "该采购确认已有相同分行序号，请刷新后重试".to_string()
        }
        Some("uk_procurement_responsibility_active_selector") => {
            "同一采购责任选择器只能有一条启用规则".to_string()
        }
        Some("uk_work_items_open_fulfillment_object") => "该履约对象已存在开放任务，请刷新后重试".to_string(),
        Some("uk_work_items_open_customer_acceptance_object") => {
            "该销售单已存在开放客户验收任务，请刷新后重试".to_string()
        }
        Some("uk_product_publication_revisions_publication_revision") => {
            "该发布修订序号已被占用，请刷新后重试".to_string()
        }
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

/// 合同冻结的结构化服务错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ApprovalPolicyNotRegistered,
    ApprovalProcessNotConfigured,
    ApprovalDraftSourceNotAvailable,
    ApprovalDefinitionNotDraft,
    ApprovalDefinitionVersionConflict,
    ApprovalDefinitionInvalid,
    ApprovalDefinitionBindingCorrupted,
    ApprovalAlreadyStarted,
    ApprovalTaskNotOpen,
    ApprovalTaskNotAssignedToActor,
    ApprovalTaskVersionConflict,
    ApprovalInstanceVersionConflict,
    ApprovalExecutionVersionConflict,
    ApprovalSubjectVersionConflict,
    ApprovalRejectReasonRequired,
    ApprovalInstanceBlocked,
    ApprovalResumeNotAllowedForBlocker,
    ApprovalCurrentApproverNotRecovered,
    ApprovalBlockedCancelNotAllowed,
    ApprovalGenericWorkItemMutationForbidden,
    ApprovalIdempotencyPayloadConflict,
}

impl From<erp_workflow::ErrorCode> for ErrorCode {
    /// Map workflow approval codes onto the remaining services error-code enum.
    fn from(code: erp_workflow::ErrorCode) -> Self {
        match code {
            erp_workflow::ErrorCode::ApprovalPolicyNotRegistered => Self::ApprovalPolicyNotRegistered,
            erp_workflow::ErrorCode::ApprovalProcessNotConfigured => Self::ApprovalProcessNotConfigured,
            erp_workflow::ErrorCode::ApprovalDraftSourceNotAvailable => Self::ApprovalDraftSourceNotAvailable,
            erp_workflow::ErrorCode::ApprovalDefinitionNotDraft => Self::ApprovalDefinitionNotDraft,
            erp_workflow::ErrorCode::ApprovalDefinitionVersionConflict => {
                Self::ApprovalDefinitionVersionConflict
            }
            erp_workflow::ErrorCode::ApprovalDefinitionInvalid => Self::ApprovalDefinitionInvalid,
            erp_workflow::ErrorCode::ApprovalDefinitionBindingCorrupted => {
                Self::ApprovalDefinitionBindingCorrupted
            }
            erp_workflow::ErrorCode::ApprovalAlreadyStarted => Self::ApprovalAlreadyStarted,
            erp_workflow::ErrorCode::ApprovalTaskNotOpen => Self::ApprovalTaskNotOpen,
            erp_workflow::ErrorCode::ApprovalTaskNotAssignedToActor => Self::ApprovalTaskNotAssignedToActor,
            erp_workflow::ErrorCode::ApprovalTaskVersionConflict => Self::ApprovalTaskVersionConflict,
            erp_workflow::ErrorCode::ApprovalInstanceVersionConflict => Self::ApprovalInstanceVersionConflict,
            erp_workflow::ErrorCode::ApprovalExecutionVersionConflict => {
                Self::ApprovalExecutionVersionConflict
            }
            erp_workflow::ErrorCode::ApprovalSubjectVersionConflict => Self::ApprovalSubjectVersionConflict,
            erp_workflow::ErrorCode::ApprovalRejectReasonRequired => Self::ApprovalRejectReasonRequired,
            erp_workflow::ErrorCode::ApprovalInstanceBlocked => Self::ApprovalInstanceBlocked,
            erp_workflow::ErrorCode::ApprovalResumeNotAllowedForBlocker => {
                Self::ApprovalResumeNotAllowedForBlocker
            }
            erp_workflow::ErrorCode::ApprovalCurrentApproverNotRecovered => {
                Self::ApprovalCurrentApproverNotRecovered
            }
            erp_workflow::ErrorCode::ApprovalBlockedCancelNotAllowed => Self::ApprovalBlockedCancelNotAllowed,
            erp_workflow::ErrorCode::ApprovalGenericWorkItemMutationForbidden => {
                Self::ApprovalGenericWorkItemMutationForbidden
            }
            erp_workflow::ErrorCode::ApprovalIdempotencyPayloadConflict => {
                Self::ApprovalIdempotencyPayloadConflict
            }
        }
    }
}

impl ErrorCode {
    /// 审批合同冻结的全部结构化错误码。
    pub const ALL: [Self; 21] = [
        Self::ApprovalPolicyNotRegistered,
        Self::ApprovalProcessNotConfigured,
        Self::ApprovalDraftSourceNotAvailable,
        Self::ApprovalDefinitionNotDraft,
        Self::ApprovalDefinitionVersionConflict,
        Self::ApprovalDefinitionInvalid,
        Self::ApprovalDefinitionBindingCorrupted,
        Self::ApprovalAlreadyStarted,
        Self::ApprovalTaskNotOpen,
        Self::ApprovalTaskNotAssignedToActor,
        Self::ApprovalTaskVersionConflict,
        Self::ApprovalInstanceVersionConflict,
        Self::ApprovalExecutionVersionConflict,
        Self::ApprovalSubjectVersionConflict,
        Self::ApprovalRejectReasonRequired,
        Self::ApprovalInstanceBlocked,
        Self::ApprovalResumeNotAllowedForBlocker,
        Self::ApprovalCurrentApproverNotRecovered,
        Self::ApprovalBlockedCancelNotAllowed,
        Self::ApprovalGenericWorkItemMutationForbidden,
        Self::ApprovalIdempotencyPayloadConflict,
    ];

    /// 返回机器可读的稳定码。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApprovalPolicyNotRegistered => "APPROVAL_POLICY_NOT_REGISTERED",
            Self::ApprovalProcessNotConfigured => "APPROVAL_PROCESS_NOT_CONFIGURED",
            Self::ApprovalDraftSourceNotAvailable => "APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE",
            Self::ApprovalDefinitionNotDraft => "APPROVAL_DEFINITION_NOT_DRAFT",
            Self::ApprovalDefinitionVersionConflict => "APPROVAL_DEFINITION_VERSION_CONFLICT",
            Self::ApprovalDefinitionInvalid => "APPROVAL_DEFINITION_INVALID",
            Self::ApprovalDefinitionBindingCorrupted => "APPROVAL_DEFINITION_BINDING_CORRUPTED",
            Self::ApprovalAlreadyStarted => "APPROVAL_ALREADY_STARTED",
            Self::ApprovalTaskNotOpen => "APPROVAL_TASK_NOT_OPEN",
            Self::ApprovalTaskNotAssignedToActor => "APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR",
            Self::ApprovalTaskVersionConflict => "APPROVAL_TASK_VERSION_CONFLICT",
            Self::ApprovalInstanceVersionConflict => "APPROVAL_INSTANCE_VERSION_CONFLICT",
            Self::ApprovalExecutionVersionConflict => "APPROVAL_EXECUTION_VERSION_CONFLICT",
            Self::ApprovalSubjectVersionConflict => "APPROVAL_SUBJECT_VERSION_CONFLICT",
            Self::ApprovalRejectReasonRequired => "APPROVAL_REJECT_REASON_REQUIRED",
            Self::ApprovalInstanceBlocked => "APPROVAL_INSTANCE_BLOCKED",
            Self::ApprovalResumeNotAllowedForBlocker => "APPROVAL_RESUME_NOT_ALLOWED_FOR_BLOCKER",
            Self::ApprovalCurrentApproverNotRecovered => "APPROVAL_CURRENT_APPROVER_NOT_RECOVERED",
            Self::ApprovalBlockedCancelNotAllowed => "APPROVAL_BLOCKED_CANCEL_NOT_ALLOWED",
            Self::ApprovalGenericWorkItemMutationForbidden => "APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN",
            Self::ApprovalIdempotencyPayloadConflict => "APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT",
        }
    }

    /// 返回不依赖 HTTP 的服务错误分类。
    pub const fn class(self) -> ErrorClass {
        match self {
            Self::ApprovalPolicyNotRegistered => ErrorClass::Internal,
            Self::ApprovalTaskNotAssignedToActor => ErrorClass::Forbidden,
            Self::ApprovalDefinitionInvalid | Self::ApprovalRejectReasonRequired => ErrorClass::BusinessRule,
            _ => ErrorClass::Conflict,
        }
    }

    /// 返回冲突后刷新并重试是否安全。
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::ApprovalDefinitionVersionConflict
                | Self::ApprovalTaskVersionConflict
                | Self::ApprovalInstanceVersionConflict
                | Self::ApprovalExecutionVersionConflict
                | Self::ApprovalSubjectVersionConflict
        )
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Error {
    /// Whether a failed transaction may already have committed on the server.
    pub fn command_may_have_committed(&self) -> bool {
        matches!(
            self,
            Self::OutcomeUnknown(_) | Self::ReceiptDuplicate(_) | Self::TransientTransaction(_)
        )
    }
    /// 由合同冻结的审批稳定码构造服务错误。
    ///
    /// `APPROVAL_POLICY_NOT_REGISTERED` 只允许作为内部错误；资格与图校验走 422 语义，
    /// 责任不匹配走 403，其余稳定码走冲突。未接入类型不得回退旧运行时。
    ///
    /// # 参数
    /// * `code` - 合同冻结的结构化错误码
    ///
    /// # 返回
    /// 返回已带稳定码的服务错误。
    pub const fn from_approval_code(code: ErrorCode) -> Self {
        Self::Coded(code)
    }

    /// 返回结构化服务错误码。
    ///
    /// # 返回
    /// 仅结构化错误返回稳定码；普通业务文案不得被反向解析。
    pub const fn code(&self) -> Option<ErrorCode> {
        match self {
            Self::Coded(code) => Some(*code),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;

    use super::{duplicate_index_conflict_message, Error, ErrorCode};
    use application_core::ErrorClass;

    #[test]
    fn optimistic_locking_error_maps_to_conflict() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);

        assert!(matches!(error, Error::ConflictError(_)));
    }

    #[test]
    fn duplicate_key_error_maps_to_conflict() {
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom(
            "duplicate key",
        )));

        assert!(matches!(error, Error::ConflictError(_)));
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn known_party_duplicate_index_maps_to_field_message() {
        let message = duplicate_index_conflict_message(Some("uk_parties_party_no"));

        assert_eq!(message, "主体编号已存在");
    }

    #[test]
    fn contract_number_duplicate_index_maps_to_contract_message() {
        let message = duplicate_index_conflict_message(Some("uk_contracts_contract_no"));

        assert_eq!(message, "合同编号已存在");
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
    fn publication_revision_duplicate_maps_to_refresh_message() {
        let message =
            duplicate_index_conflict_message(Some("uk_product_publication_revisions_publication_revision"));

        assert_eq!(message, "该发布修订序号已被占用，请刷新后重试");
    }

    #[test]
    fn transient_transaction_error_maps_to_conflict() {
        let error = Error::from(persistence_core::Error::TransientTransactionConflict(
            MongoError::custom("write conflict"),
        ));

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
        let error = Error::from(persistence_core::Error::DatabaseError(MongoError::custom(
            "connection failed",
        )));

        assert!(matches!(error, Error::RepositoryError(_)));
    }

    #[test]
    fn unknown_commit_outcome_has_dedicated_service_semantics() {
        let error = Error::from(persistence_core::Error::CommitOutcomeUnknown(MongoError::custom(
            "unknown commit",
        )));

        assert!(matches!(&error, Error::OutcomeUnknown(_)));
        assert_eq!(
            error.to_string(),
            "操作结果暂无法确认，请查询当前状态后再决定是否重试"
        );
    }

    #[test]
    fn approval_policy_not_registered_is_internal() {
        let error = Error::from_approval_code(ErrorCode::ApprovalPolicyNotRegistered);
        assert_eq!(error.code(), Some(ErrorCode::ApprovalPolicyNotRegistered));
        assert_eq!(error.code().expect("code").class(), ErrorClass::Internal);
    }

    #[test]
    fn contract_unprocessable_codes_are_business_logic_not_validation() {
        for code in [
            ErrorCode::ApprovalDefinitionInvalid,
            ErrorCode::ApprovalRejectReasonRequired,
        ] {
            let error = Error::from_approval_code(code);
            assert_eq!(code.class(), ErrorClass::BusinessRule, "{code} 必须是 422 语义");
            assert_eq!(error.code(), Some(code));
        }
    }

    #[test]
    fn approval_stable_codes_are_exhaustive() {
        assert_eq!(ErrorCode::ALL.len(), 21);
        assert!(ErrorCode::ALL
            .iter()
            .all(|code| code.as_str().starts_with("APPROVAL_")));
    }
}
