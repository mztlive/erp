//! 在事务已返回之后记录独立尝试，并保持首次业务错误对象。

use application_core::ErrorClass;
use async_trait::async_trait;
use erp_audit::{AuditAttempt, AuditAttemptExt, AuditAttemptResult, BusinessEventContext};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::{Error, Result};

/// 失败、拒绝及未知尝试的事务外窄持久化端口。
#[async_trait]
pub trait AuditAttemptSink: Send + Sync {
    /// 保存安全尝试快照，不参与原事务且不解释业务回执。
    /// # 参数
    /// * `attempt` - 类型化最小上下文，不含错误正文。
    /// # 返回
    /// 返回尝试写入结果。
    /// # 错误
    /// 尝试记录失败时返回错误；调用方仍须保留原命令错误。
    async fn persist_attempt(&self, attempt: &AuditAttempt) -> Result<()>;
}

/// 使用独立 NoTransaction 写入尝试集合。
pub struct MongoAuditAttemptSink<'a>(&'a Database);
impl<'a> MongoAuditAttemptSink<'a> {
    /// 绑定原命令数据库，不复用已经结束的事务执行器。
    ///
    /// # 参数
    /// * `db` - 审计领域所在数据库。
    ///
    /// # 返回
    /// 返回独立尝试写入器。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: &'a Database) -> Self {
        Self(db)
    }
}
#[async_trait]
impl AuditAttemptSink for MongoAuditAttemptSink<'_> {
    async fn persist_attempt(&self, attempt: &AuditAttempt) -> Result<()> {
        self.0.audit_attempts().create(attempt, &mut NoTransaction).await?;
        Ok(())
    }
}

/// 对原错误进行结构化分类，不解析 Display 或复制敏感错误正文。
fn attempt_result(error: &Error) -> AuditAttemptResult {
    match error {
        Error::OutcomeUnknown(_) => AuditAttemptResult::Unknown,
        Error::ValidationError(_)
        | Error::BusinessLogicError(_)
        | Error::NotFound(_)
        | Error::Forbidden(_)
        | Error::Unauthenticated(_) => AuditAttemptResult::Rejected,
        Error::Coded(code) if code.class() == ErrorClass::Internal => AuditAttemptResult::Failed,
        Error::Coded(_) => AuditAttemptResult::Rejected,
        _ => AuditAttemptResult::Failed,
    }
}

/// 在事务完成后补记尝试；写尝试失败不覆盖首次错误或重执行命令。
/// # 参数
/// * `result` - 原业务事务返回值，错误对象按原值保留。
/// * `context` - 执行前已验证的静态、安全关联上下文。
/// * `sink` - 与原业务事务分开的尝试持久化端口。
/// # 返回
/// 返回原命令结果；成功不追加尝试事件。
/// # 错误
/// 仅返回原错误。尝试写入故障记录固定安全诊断，不替换首次错误。
pub async fn finish_attempt<T, S: AuditAttemptSink>(
    result: Result<T>,
    context: &BusinessEventContext,
    sink: &S,
) -> Result<T> {
    if let Err(error) = &result {
        let attempt = context.attempt(attempt_result(error));
        if sink.persist_attempt(&attempt).await.is_err() {
            tracing::warn!(event_kind = "command_attempt", action_code = %attempt.action_code,
                result = ?attempt.result, "命令尝试审计写入失败");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use application_core::AuditActor;
    use erp_audit::registered_action;
    use erp_core::AccountKind;

    use super::*;
    struct Sink {
        fail: bool,
        attempts: Mutex<Vec<AuditAttempt>>,
    }
    #[async_trait]
    impl AuditAttemptSink for Sink {
        async fn persist_attempt(&self, attempt: &AuditAttempt) -> Result<()> {
            self.attempts.lock().unwrap().push(attempt.clone());
            if self.fail {
                return Err(Error::Internal("attempt unavailable".into()));
            }
            Ok(())
        }
    }
    fn context() -> BusinessEventContext {
        BusinessEventContext::new(
            AuditActor::new("actor".into(), "account".into(), AccountKind::Admin),
            registered_action("customer.update", "customer").unwrap(),
        )
        .unwrap()
        .with_command_id(Some("command".into()))
        .unwrap()
        .with_target(Some("customer".into()), Some("KH-001".into()))
        .unwrap()
    }
    #[tokio::test]
    async fn attempt_failure_preserves_unknown_source_without_success_event_or_reexecution() {
        let sink = Sink { fail: true, attempts: Mutex::new(Vec::new()) };
        let result: Result<()> = finish_attempt(
            Err(Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(
                mongodb::error::Error::custom("original unknown"),
            ))),
            &context(),
            &sink,
        )
        .await;
        match result.unwrap_err() {
            Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(source)) => {
                assert_eq!(source.get_custom::<&str>(), Some(&"original unknown"));
            },
            error => panic!("原错误被替换: {error:?}"),
        }
        let attempts = sink.attempts.lock().unwrap();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].result, AuditAttemptResult::Unknown);
        assert_eq!(attempts[0].command_id.as_deref(), Some("command"));
        assert_eq!(attempts[0].resource_number_snapshot.as_deref(), Some("KH-001"));
        assert!(!serde_json::to_string(&attempts[0]).unwrap().contains("original unknown"));
    }
    #[tokio::test]
    async fn attempted_rejection_failure_and_success_have_distinct_recording() {
        let sink = Sink { fail: false, attempts: Mutex::new(Vec::new()) };
        for error in
            [Error::Forbidden("secret authorization detail".into()), Error::Internal("raw request".into())]
        {
            let result: Result<()> = finish_attempt(Err(error), &context(), &sink).await;
            assert!(result.is_err());
        }
        assert_eq!(finish_attempt(Ok(7), &context(), &sink).await.unwrap(), 7);
        let attempts = sink.attempts.lock().unwrap();
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].result, AuditAttemptResult::Rejected);
        assert_eq!(attempts[1].result, AuditAttemptResult::Failed);
        assert_ne!(attempts[0].base.id, attempts[1].base.id);
        let wire = serde_json::to_string(&*attempts).unwrap();
        assert!(!wire.contains("raw request"));
        assert!(!wire.contains("secret authorization"));
    }
}
