//! Workflow-domain application errors and frozen approval ErrorCode.

use application_core::ErrorClass;

/// Workflow-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为工作流领域错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

/// Workflow, approval and work-item errors.
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

impl Error {
    /// Stable error class used by HTTP mapping; do not parse display text.
    pub fn class(&self) -> ErrorClass {
        match self {
            Self::Internal(_) | Self::Logic(_) | Self::Rbac(_) | Self::RepositoryError(_) => {
                ErrorClass::Internal
            }
            Self::ConflictError(_) | Self::ReceiptDuplicate(_) | Self::TransientTransaction(_) => {
                ErrorClass::Conflict
            }
            Self::BusinessLogicError(_) | Self::ValidationError(_) | Self::NotFound(_) => {
                ErrorClass::BusinessRule
            }
            Self::Forbidden(_) | Self::Unauthenticated(_) => ErrorClass::Forbidden,
            Self::OutcomeUnknown(_) => ErrorClass::Internal,
            Self::Coded(code) => code.class(),
        }
    }

    /// 由合同冻结的审批稳定码构造工作流错误。
    ///
    /// # 参数
    /// * `code` - 合同冻结的结构化错误码
    ///
    /// # 返回
    /// 返回已带稳定码的工作流错误。
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

impl From<persistence_core::Error> for Error {
    /// 将仓储错误转换为工作流领域错误。
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

fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    match error.duplicate_index_name() {
        Some("uk_work_items_open_fulfillment_object") => "该履约对象已存在开放任务，请刷新后重试".to_string(),
        Some("uk_work_items_open_customer_acceptance_object") => {
            "该销售单已存在开放客户验收任务，请刷新后重试".to_string()
        }
        _ => "数据已存在，请勿重复提交".to_string(),
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建工作流领域错误。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}

/// 合同冻结的结构化审批错误码。
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

#[cfg(test)]
mod tests {
    use super::{Error, ErrorCode};
    use application_core::ErrorClass;

    #[test]
    fn from_approval_code_preserves_stable_code() {
        let error = Error::from_approval_code(ErrorCode::ApprovalPolicyNotRegistered);
        assert_eq!(error.code(), Some(ErrorCode::ApprovalPolicyNotRegistered));
        assert_eq!(error.class(), ErrorClass::Internal);
    }

    #[test]
    fn frozen_error_codes_cover_contract_set() {
        assert_eq!(ErrorCode::ALL.len(), 21);
        assert!(ErrorCode::ALL.contains(&ErrorCode::ApprovalGenericWorkItemMutationForbidden));
    }
}
