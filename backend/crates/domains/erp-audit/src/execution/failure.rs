//! 消费方错误对审计结果和原命令恢复的最小分类合同。

use crate::{AuditAttemptResult, Error};

/// 只提供安全分类，不传递业务错误正文或解释业务回执。
pub trait AuditFailure {
    /// 返回失败尝试的安全结果分类。
    /// # 参数
    /// 无。
    /// # 返回
    /// 拒绝、未知或失败分类。
    /// # 错误
    /// 不返回错误。
    fn audit_attempt_result(&self) -> AuditAttemptResult;

    /// 判断原事务提交是否无法确认。
    /// # 参数
    /// 无。
    /// # 返回
    /// 仅提交结果未知时返回 `true`。
    /// # 错误
    /// 不返回错误。
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
