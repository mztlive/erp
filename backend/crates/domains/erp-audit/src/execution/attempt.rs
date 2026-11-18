//! 在事务已返回之后记录独立尝试，并保持首次业务错误对象。

use async_trait::async_trait;
use mongodb::Database;
use persistence_core::NoTransaction;

use super::AuditFailure;
use crate::{AuditAttempt, AuditAttemptExt, BusinessEventContext, Result};
#[cfg(test)]
use crate::{AuditAttemptResult, Error};

/// 失败、拒绝及未知尝试的事务外窄持久化端口。
#[async_trait]
pub trait AuditAttemptSink: Send + Sync {
    /// 在原事务之外保存失败、拒绝或未知的最小安全上下文，不记录错误正文或解释业务回执。保存失败返回错误，但调用方仍须保留原命令错误。
    async fn persist_attempt(&self, attempt: &AuditAttempt) -> Result<()>;
}

/// 使用独立 NoTransaction 写入尝试集合。
pub struct MongoAuditAttemptSink<'a>(&'a Database);
impl<'a> MongoAuditAttemptSink<'a> {
    /// 绑定尝试记录使用的数据库，供原事务结束后单独写入安全留痕。
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

/// 在原事务结束后补记失败、拒绝或未知尝试，成功时直接返回原结果。补记异常只写固定安全诊断，不覆盖首次错误，也不重新执行业务。
pub async fn finish_attempt<T, E: AuditFailure, S: AuditAttemptSink>(
    result: std::result::Result<T, E>,
    context: &BusinessEventContext,
    sink: &S,
) -> std::result::Result<T, E> {
    if let Err(error) = &result {
        let attempt = context.attempt(error.audit_attempt_result());
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
    use erp_core::AccountKind;

    use super::*;
    use crate::registered_action;
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
        assert_eq!(finish_attempt(Ok::<_, Error>(7), &context(), &sink).await.unwrap(), 7);

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
