//! 应用边界拥有的错误类型；领域错误载荷与命令恢复分类保持冻结合同。

use application_core::{ErrorClass, ValidationIssues};
use erp_audit::AuditAttemptResult;
use erp_audit::execution::AuditFailure;
use erp_permissions::ParsePermissionError;
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

from_domain!(erp_identity, Rbac);
from_domain!(erp_supplier_catalog, Rbac, Coded);
from_domain!(erp_workflow, Rbac, Coded);
from_domain!(erp_party);
from_domain!(erp_customer);
from_domain!(erp_supplier);
from_domain!(erp_support);
from_domain!(erp_audit);
from_domain!(erp_catalog);
from_domain!(erp_warehouse);
from_domain!(erp_supply);

from_domain!(erp_procurement);

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

// 向资料、供给或工作流的索引拥有方查询冲突提示；无名或未登记索引统一按重复提交提示返回。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name
        .and_then(|name| {
            erp_party::known_duplicate_index_message(name)
                .or_else(|| erp_supplier::known_duplicate_index_message(name))
                .or_else(|| erp_supply::known_duplicate_index_message(name))
                .or_else(|| erp_procurement::known_duplicate_index_message(name))
                .or_else(|| erp_customer::known_duplicate_index_message(name))
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

// Process 调用读模型时按全部 14 个 variant 穷尽接收；读模型没有反向依赖。

from_domain!(erp_master_data, Rbac);

from_domain!(erp_procure_to_pay, Rbac, Coded);

impl From<ParsePermissionError> for Error {
    fn from(value: ParsePermissionError) -> Self {
        Self::ValidationError(value.to_string())
    }
}
