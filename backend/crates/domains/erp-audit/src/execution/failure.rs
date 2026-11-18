//! 消费方错误对审计结果和原命令恢复的最小分类合同。

use crate::{AuditAttemptResult, Error};

/// 只提供安全分类，不传递业务错误正文或解释业务回执。
pub trait AuditFailure {
    /// 将原失败映射成拒绝、失败或结果未知的安全分类，供事务外尝试记录使用。
    fn audit_attempt_result(&self) -> AuditAttemptResult;

    /// 仅在首次事务提交结果无法确认时返回 `true`，供查证路径决定是否保留原错误。
    fn outcome_unknown(&self) -> bool;
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
            _ => AuditAttemptResult::Failed,
        }
    }

    fn outcome_unknown(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_))
    }
}
