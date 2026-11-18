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
    // 把应用合同层的失败转换为履约执行错误，供命令入口按原错误类别处理。
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
from_domain!(erp_approval_runtime, Rbac, Coded);
from_domain!(erp_file_assets);
from_domain!(erp_workflow, Rbac, Coded);
from_domain!(erp_party);
from_domain!(erp_supplier);
from_domain!(erp_support);
from_domain!(erp_audit);
from_domain!(erp_warehouse);
from_domain!(erp_inventory);
from_domain!(erp_finance);
from_domain!(erp_integration);
from_domain!(erp_supply);
impl From<erp_fulfillment::Error> for Error {
    // 履约编排处理履约失败时保持原错误类别。履约不再返回 `ReceiptDuplicate`；
    // 唯一冲突按 `uk_*` 索引细化为 `ConflictError`，这里仍按冲突向上层传播。
    fn from(error: erp_fulfillment::Error) -> Self {
        match error {
            erp_fulfillment::Error::Internal(payload) => Self::Internal(payload),
            erp_fulfillment::Error::NotFound(payload) => Self::NotFound(payload),
            erp_fulfillment::Error::ValidationError(payload) => Self::ValidationError(payload),
            erp_fulfillment::Error::InvalidInput(payload) => Self::InvalidInput(payload),
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

impl From<erp_sales::Error> for Error {
    // 履约编排调用销售领域时保留原错误语义；选品专用变体沿用冲突、业务或内部错误分类。
    fn from(error: erp_sales::Error) -> Self {
        match error {
            erp_sales::Error::Internal(payload) => Self::Internal(payload),
            erp_sales::Error::NotFound(payload) => Self::NotFound(payload),
            erp_sales::Error::ValidationError(payload) => Self::ValidationError(payload),
            erp_sales::Error::InvalidInput(payload) => Self::InvalidInput(payload),
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

impl From<persistence_core::Error> for Error {
    // 持久化错误跨出编排时保留原分类和错误内容，只有历史归档索引的提示交给 HTTP 兼容分支。
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

// 映射唯一冲突时只识别由 HTTP 兼容层处理的三个历史索引；这里不生成文案，也不扩大命令恢复范围。
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

// 生成唯一键冲突提示时只采用索引所属领域的已知文案；无名或未知索引仍返回通用冲突提示。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name
        .and_then(|name| {
            erp_party::known_duplicate_index_message(name)
                .or_else(|| erp_supplier::known_duplicate_index_message(name))
                .or_else(|| erp_supply::known_duplicate_index_message(name))
                .or_else(|| erp_procurement::known_duplicate_index_message(name))
                .or_else(|| erp_workflow::known_duplicate_index_message(name))
        })
        .unwrap_or("数据已存在，请勿重复提交")
        .to_string()
}

impl From<validator::ValidationErrors> for Error {
    // 履约执行命令校验输入失败时，将字段校验详情归入 `InvalidInput`。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::InvalidInput(err.into())
    }
}

impl Error {
    /// 供调用方判断履约执行命令失败后是否仍可能已经提交。
    /// `OutcomeUnknown`、`ReceiptDuplicate` 或 `TransientTransaction` 时返回 `true`，其余返回 `false`。
    pub fn command_may_have_committed(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_) | Self::ReceiptDuplicate(_) | Self::TransientTransaction(_))
    }
    /// 将审批运行时的稳定错误码带出履约执行入口，用 `Error::Coded` 原样保留。
    ///
    /// `APPROVAL_POLICY_NOT_REGISTERED` 归内部错误，未指派给操作人归拒绝，定义非法或驳回原因缺失归业务规则，其余归冲突。
    pub const fn from_approval_code(code: WorkflowErrorCode) -> Self {
        Self::Coded(code)
    }

    /// 供上层读取审批稳定码；只有 `Error::Coded` 返回码，普通业务文案返回 `None`，不做反向解析。
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
from_domain!(erp_read_models, Rbac, Coded);
impl erp_approval_runtime::approval_recovery::ApprovalCommandFailure for Error {
    fn command_may_have_committed(&self) -> bool {
        self.command_may_have_committed()
    }
}
