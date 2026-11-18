//! 类型化业务事件执行边界；复用调用方 Executor，不承担业务规则或命令身份恢复。

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use mongodb::Database;
use persistence_core::Executor;

use crate::{
    AuditExt, AuditLog, AuditLogRepositoryExt, BusinessEventContent, BusinessEventContext,
    BusinessEventResult, Error, Result, prepare_business_log,
};

/// 一次业务命令的执行结果；回放结果不会再次生成成功事件。
pub enum AuditedWrite<T> {
    /// 首次成功执行业务命令及其允许记录的结果投影。
    Fresh { result: T, content: BusinessEventContent },
    /// 由拥有领域恢复的原命令结果。
    Replayed(T),
}

/// 拥有领域或命名 Process 声明的实际业务执行边界。
#[async_trait]
pub trait AuditedCommand: Send + Sync {
    type Output: Send;
    type Error: From<Error> + Send;

    /// 在调用方的同一事务执行器内完成领域校验、写入或回执恢复。首次成功携带安全事件投影，回放只返回原结果；授权、校验、版本和仓储错误沿用原分类，不重试写入。
    async fn execute(
        &self,
        executor: &mut dyn Executor,
    ) -> std::result::Result<AuditedWrite<Self::Output>, Self::Error>;
}

/// 成功业务事件的同事务持久化端口。
#[async_trait]
pub trait AuditEventSink: Send + Sync {
    type Error: Send;
    /// 用业务写入的原执行器保存已通过静态与结果校验的事件，确保正式结果和审计同事务提交。写入失败返回原错误，调用方不得提交事务。
    async fn persist(
        &self,
        log: &AuditLog,
        executor: &mut dyn Executor,
    ) -> std::result::Result<(), Self::Error>;
}

/// 组合层将类型化事件端口接到审计领域仓储。
pub struct MongoAuditEventSink<'a> {
    db: &'a Database,
}

impl<'a> MongoAuditEventSink<'a> {
    /// 绑定当前业务数据库，把成功事件端口接到审计仓储；写入时复用调用方传入的执行器。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AuditEventSink for MongoAuditEventSink<'_> {
    type Error = Error;
    async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
        persist_log(self.db, log, executor).await
    }
}

/// 先验证已准备的审计记录及成功标记，再在原执行器调用不含外部 I/O 的业务闭包，最后以同一执行器保存审计。只有两次写入均成功才返回业务结果；遇到首个错误即停止，不重执行。
pub async fn execute_prepared<R, E, S, F>(
    audit: &AuditLog,
    sink: &S,
    executor: &mut dyn Executor,
    write: F,
) -> std::result::Result<R, E>
where
    S: AuditEventSink,
    E: From<Error> + From<S::Error>,
    F: for<'a> FnOnce(
        &'a mut dyn Executor,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<R, E>> + Send + 'a>>,
{
    let prepared = prepare_business_log(audit)?;
    if !prepared.success {
        return Err(Error::ValidationError("操作结果与审计记录不一致，请联系管理员".into()).into());
    }

    let result = write(executor).await?;
    sink.persist(&prepared, executor).await?;
    Ok(result)
}

/// 验证登记动作和身份后，用正式业务的原执行器写入普通审计，不另开事务或重试。登记、身份或仓储错误原样返回，调用方不得提交。
pub async fn persist_log(db: &Database, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
    let prepared = prepare_business_log(log)?;
    db.audit_logs().create(&prepared, executor).await?;
    Ok(())
}

/// 先逐条验证整批日志，再以原业务执行器按原顺序写入；任一动作没有登记、身份无效或批量入库失败时返回原错误。
pub async fn persist_logs(db: &Database, logs: &[AuditLog], executor: &mut dyn Executor) -> Result<()> {
    let prepared = logs.iter().map(prepare_business_log).collect::<Result<Vec<_>>>()?;
    db.audit_logs().create_many_ordered(&prepared, executor).await?;
    Ok(())
}

/// 在调用方现有事务中使用提前校验过身份和动作的上下文执行类型化命令；首次成功时验证投影并写一次审计，回放则只返回原结果。
/// 业务、投影或入库失败时直接返回错误；本入口不开事务、不重试，也不写独立失败尝试。
pub async fn execute_audited<C: AuditedCommand, S: AuditEventSink>(
    context: &BusinessEventContext,
    sink: &S,
    executor: &mut dyn Executor,
    command: &C,
) -> std::result::Result<C::Output, C::Error>
where
    C::Error: From<S::Error>,
{
    match command.execute(executor).await? {
        AuditedWrite::Replayed(result) => Ok(result),
        AuditedWrite::Fresh { result, content } => {
            if content.result != BusinessEventResult::Succeeded {
                return Err(
                    Error::ValidationError("操作结果与审计记录不一致，请联系管理员".to_string()).into()
                );
            }

            let log = context.log(content)?;
            sink.persist(&log, executor).await?;
            Ok(result)
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;
    use crate::{AuditAction, AuditFact, AuditValue};

    const ACTION: AuditAction = AuditAction {
        code: "test.confirm",
        resource_type: "test_record",
        label: "确认测试记录",
        version: 1,
        allowed_fields: &[],
    };

    use persistence_core::NoTransaction;

    struct RecordingBoundary {
        identity: usize,
        fail_write: bool,
        unknown_write: bool,
        fail_audit: bool,
        replay: bool,
        invalid_content: bool,
        result: BusinessEventResult,
        calls: Mutex<Vec<&'static str>>,
        logs: Mutex<Vec<AuditLog>>,
    }

    impl RecordingBoundary {
        fn visit(&self, executor: &mut dyn Executor, step: &'static str) {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.identity);
            self.calls.lock().unwrap().push(step);
        }
    }

    #[async_trait]
    impl AuditedCommand for RecordingBoundary {
        type Error = Error;
        type Output = u32;

        async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<u32>> {
            self.visit(executor, "execute");
            if self.fail_write {
                return Err(Error::ConflictError("原业务错误".to_string()));
            }
            if self.unknown_write {
                return Err(Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(
                    MongoError::custom("original unknown commit"),
                )));
            }
            if self.replay {
                return Ok(AuditedWrite::Replayed(17));
            }
            Ok(AuditedWrite::Fresh {
                result: 17,
                content: BusinessEventContent {
                    target_id: "record-1".to_string(),
                    target_number: Some("REC-001".to_string()),
                    result: self.result,
                    field_changes: Vec::new(),
                    facts: if self.invalid_content {
                        vec![AuditFact { field: "bank_account".to_string(), value: AuditValue::Changed }]
                    } else {
                        Vec::new()
                    },
                },
            })
        }
    }

    #[async_trait]
    impl AuditEventSink for RecordingBoundary {
        type Error = Error;
        async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
            self.visit(executor, "audit");
            if self.fail_audit {
                return Err(Error::ConflictError("原审计错误".to_string()));
            }
            self.logs.lock().unwrap().push(log.clone());
            Ok(())
        }
    }

    fn context() -> BusinessEventContext {
        BusinessEventContext::new(
            AuditActor::new("actor-1".to_string(), "caigou".to_string(), AccountKind::Admin),
            ACTION,
        )
        .unwrap()
    }

    fn boundary(executor: &mut NoTransaction) -> RecordingBoundary {
        RecordingBoundary {
            identity: executor as *mut NoTransaction as usize,
            fail_write: false,
            unknown_write: false,
            fail_audit: false,
            replay: false,
            invalid_content: false,
            result: BusinessEventResult::Succeeded,
            calls: Mutex::new(Vec::new()),
            logs: Mutex::new(Vec::new()),
        }
    }

    #[tokio::test]
    async fn fresh_execution_records_once_and_passes_same_executor() {
        let mut executor = NoTransaction;
        let boundary = boundary(&mut executor);
        let context = context();
        assert_eq!(execute_audited(&context, &boundary, &mut executor, &boundary).await.unwrap(), 17);
        assert_eq!(*boundary.calls.lock().unwrap(), ["execute", "audit"]);

        let logs = boundary.logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].base.id, context.event_id());
        assert_eq!(
            logs[0].structured_event.as_ref().unwrap().resource_number_snapshot.as_deref(),
            Some("REC-001")
        );
    }

    #[tokio::test]
    async fn replay_does_not_write_a_second_business_event() {
        let mut executor = NoTransaction;
        let mut boundary = boundary(&mut executor);
        boundary.replay = true;
        assert_eq!(execute_audited(&context(), &boundary, &mut executor, &boundary).await.unwrap(), 17);
        assert_eq!(*boundary.calls.lock().unwrap(), ["execute"]);
        assert!(boundary.logs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn original_business_and_audit_errors_stop_execution() {
        for (fail_write, expected, steps) in
            [(true, "原业务错误", vec!["execute"]), (false, "原审计错误", vec!["execute", "audit"])]
        {
            let mut executor = NoTransaction;
            let mut boundary = boundary(&mut executor);
            boundary.fail_write = fail_write;
            boundary.fail_audit = !fail_write;
            let error = execute_audited(&context(), &boundary, &mut executor, &boundary).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == expected));
            assert_eq!(*boundary.calls.lock().unwrap(), steps);
            assert!(boundary.logs.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn invalid_result_projection_stops_before_audit_write() {
        let mut executor = NoTransaction;
        let mut boundary = boundary(&mut executor);
        boundary.invalid_content = true;
        assert!(execute_audited(&context(), &boundary, &mut executor, &boundary).await.is_err());
        assert_eq!(*boundary.calls.lock().unwrap(), ["execute"]);
    }

    #[tokio::test]
    async fn unknown_or_rejected_is_not_relabelled_as_success() {
        for result in [BusinessEventResult::Unknown, BusinessEventResult::Rejected] {
            let mut executor = NoTransaction;
            let mut boundary = boundary(&mut executor);
            boundary.result = result;
            assert!(execute_audited(&context(), &boundary, &mut executor, &boundary).await.is_err());
            assert_eq!(*boundary.calls.lock().unwrap(), ["execute"]);
        }
    }

    #[tokio::test]
    async fn unknown_business_outcome_preserves_original_source_and_never_reexecutes() {
        let mut executor = NoTransaction;
        let mut boundary = boundary(&mut executor);
        boundary.unknown_write = true;
        let error = execute_audited(&context(), &boundary, &mut executor, &boundary).await.unwrap_err();
        match error {
            Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) => {
                assert_eq!(source.get_custom::<&str>(), Some(&"original unknown commit"));
            },
            other => panic!("未知业务结果分类被更改: {other:?}"),
        }

        assert_eq!(*boundary.calls.lock().unwrap(), ["execute"]);
        assert!(boundary.logs.lock().unwrap().is_empty());
    }
}

#[cfg(test)]
mod ordinary_boundary_tests {
    use std::sync::{Arc, Mutex};

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::AuditActorLogs;
    struct Sink {
        identity: usize,
        fail: bool,
        calls: Arc<Mutex<Vec<&'static str>>>,
        events: Mutex<Vec<AuditLog>>,
    }
    #[async_trait]
    impl AuditEventSink for Sink {
        type Error = Error;
        async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.identity);
            self.calls.lock().unwrap().push("event");
            if self.fail {
                return Err(Error::Internal("original event failure".into()));
            }
            self.events.lock().unwrap().push(log.clone());
            Ok(())
        }
    }
    fn log() -> AuditLog {
        AuditActor::new("actor".into(), "account".into(), AccountKind::Admin)
            .resource_log("customer.update", "customer", "customer".into())
            .unwrap()
    }
    #[tokio::test]
    async fn ordinary_production_wrapper_validates_before_write_and_keeps_same_executor() {
        for invalid in [false, true] {
            let mut executor = NoTransaction;
            let identity = &mut executor as *mut NoTransaction as usize;
            let sink = Sink {
                identity,
                fail: false,
                calls: Arc::new(Mutex::new(Vec::new())),
                events: Mutex::new(Vec::new()),
            };
            let mut audit = log();
            if invalid {
                audit.action = "unregistered.action".into();
            }
            let calls = Arc::clone(&sink.calls);
            let result: Result<u32> = execute_prepared(&audit, &sink, &mut executor, move |executor| {
                Box::pin(async move {
                    assert_eq!(executor as *mut dyn Executor as *mut () as usize, identity);
                    calls.lock().unwrap().push("write");
                    Ok(7)
                })
            })
            .await;

            if invalid {
                assert!(result.is_err());
                assert!(sink.calls.lock().unwrap().is_empty());
            } else {
                assert_eq!(result.unwrap(), 7);
                assert_eq!(*sink.calls.lock().unwrap(), ["write", "event"]);
            }
        }
    }
    #[tokio::test]
    async fn ordinary_write_and_audit_failures_keep_first_error_and_stop() {
        for fail_write in [false, true] {
            let mut executor = NoTransaction;
            let identity = &mut executor as *mut NoTransaction as usize;
            let sink = Sink {
                identity,
                fail: !fail_write,
                calls: Arc::new(Mutex::new(Vec::new())),
                events: Mutex::new(Vec::new()),
            };
            let calls = Arc::clone(&sink.calls);
            let result: Result<()> = execute_prepared(&log(), &sink, &mut executor, move |_| {
                Box::pin(async move {
                    calls.lock().unwrap().push("write");
                    if fail_write { Err(Error::Internal("original write failure".into())) } else { Ok(()) }
                })
            })
            .await;

            assert!(
                matches!(result, Err(Error::Internal(message)) if message == if fail_write { "original write failure" } else { "original event failure" })
            );
            assert_eq!(
                *sink.calls.lock().unwrap(),
                if fail_write { vec!["write"] } else { vec!["write", "event"] }
            );
            assert!(sink.events.lock().unwrap().is_empty());
        }
    }
    #[derive(Debug)]
    enum ConsumerError {
        Business(&'static str),
        Audit(Error),
    }

    impl From<Error> for ConsumerError {
        fn from(error: Error) -> Self {
            Self::Audit(error)
        }
    }

    #[tokio::test]
    async fn consumer_error_remains_typed_and_sink_error_converts_only_at_the_boundary() {
        for fail_write in [false, true] {
            let mut executor = NoTransaction;
            let sink = Sink {
                identity: &mut executor as *mut NoTransaction as usize,
                fail: true,
                calls: Arc::new(Mutex::new(Vec::new())),
                events: Mutex::new(Vec::new()),
            };
            let result = execute_prepared(&log(), &sink, &mut executor, move |_| {
                Box::pin(
                    async move { if fail_write { Err(ConsumerError::Business("original")) } else { Ok(7) } },
                )
            })
            .await;

            match (fail_write, result) {
                (true, Err(ConsumerError::Business(message))) => {
                    assert_eq!(message, "original");
                    assert!(sink.calls.lock().unwrap().is_empty());
                },
                (false, Err(ConsumerError::Audit(Error::Internal(message)))) => {
                    assert_eq!(message, "original event failure");
                    assert_eq!(*sink.calls.lock().unwrap(), ["event"]);
                },
                outcome => panic!("consumer boundary changed the error: {outcome:?}"),
            }
        }
    }
}
