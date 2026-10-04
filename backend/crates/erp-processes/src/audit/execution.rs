//! 类型化业务事件执行边界；复用调用方 Executor，不承担业务规则或命令身份恢复。

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use erp_audit::{
    AuditExt, AuditLog, AuditLogRepositoryExt, BusinessEventContent, BusinessEventContext,
    BusinessEventResult, Result as AuditResult, prepare_business_log,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

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

    /// 在已有执行器中执行业务规则、写入和结果恢复。
    ///
    /// # 参数
    /// * `executor` - 调用方传入的同一事务执行器。
    ///
    /// # 返回
    /// 首次执行返回安全事件投影；回放返回原业务结果。
    ///
    /// # 错误
    /// 沿用领域校验、授权、版本及持久化错误，不重试失败的业务写入。
    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>>;
}

/// 成功业务事件的同事务持久化端口。
#[async_trait]
pub trait AuditEventSink: Send + Sync {
    /// 持久化已经完成静态和结果校验的审计记录。
    ///
    /// # 参数
    /// * `log` - 已验证的结构化业务审计记录。
    /// * `executor` - 正式业务写入使用的同一执行器。
    ///
    /// # 返回
    /// 成功写入时返回 `Ok(())`。
    ///
    /// # 错误
    /// 审计写入失败时返回原错误，调用方事务不得提交。
    async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()>;
}

/// 组合层将类型化事件端口接到审计领域仓储。
pub struct MongoAuditEventSink<'a> {
    db: &'a Database,
}

impl<'a> MongoAuditEventSink<'a> {
    /// 绑定当前命令使用的数据库。
    ///
    /// # 参数
    /// * `db` - 当前业务用例数据库。
    ///
    /// # 返回
    /// 返回复用调用方执行器的审计写入适配器。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AuditEventSink for MongoAuditEventSink<'_> {
    async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
        persist_log(self.db, log, executor).await.map_err(Into::into)
    }
}

/// 在原执行器内执行普通已登记写入，业务成功后统一保存安全事件。
/// # 参数
/// * `audit` - 事务开始前已准备的登记元数据。
/// * `sink` - 原事务中的统一审计写入端口。
/// * `executor` - 原正式业务执行器。
/// * `write` - 实际业务闭包，不包含外部I/O。
/// # 返回
/// 返回业务结果，审计成功后才能提交。
/// # 错误
/// 元数据先校验；业务或审计首错停止并保持原错误，不重执行。
pub async fn execute_prepared<R, S, F>(
    audit: &AuditLog,
    sink: &S,
    executor: &mut dyn Executor,
    write: F,
) -> Result<R>
where
    S: AuditEventSink,
    F: for<'a> FnOnce(&'a mut dyn Executor) -> Pin<Box<dyn Future<Output = Result<R>> + Send + 'a>>,
{
    let prepared = prepare_business_log(audit)?;
    if !prepared.success {
        return Err(Error::ValidationError("成功业务边界不能提交失败审计".into()));
    }
    let result = write(executor).await?;
    sink.persist(&prepared, executor).await?;
    Ok(result)
}

/// 普通写入的唯一审计持久化入口，不建立事务或追加重试。
/// # 参数
/// * `db` - 当前业务事务数据库。
/// * `log` - 已登记静态元数据及明确白名单投影。
/// * `executor` - 原正式业务执行器。
/// # 返回
/// 返回审计写入结果。
/// # 错误
/// 未登记动作、身份投影无效或持久化失败时返回原错误并阻止提交。
pub async fn persist_log(db: &Database, log: &AuditLog, executor: &mut dyn Executor) -> AuditResult<()> {
    let prepared = prepare_business_log(log)?;
    db.audit_logs().create(&prepared, executor).await?;
    Ok(())
}

/// 有序批量保存普通事件，任何静态元数据无效时整个审计批次停止。
/// # 参数
/// * `db` - 当前业务事务数据库。
/// * `logs` - 调用方原顺序的事件集合。
/// * `executor` - 先前业务批次使用的原执行器。
/// # 返回
/// 返回原有有序批量写入结果。
/// # 错误
/// 任一事件未登记、身份无效或批量持久化失败时返回原错误。
pub async fn persist_logs(db: &Database, logs: &[AuditLog], executor: &mut dyn Executor) -> AuditResult<()> {
    let prepared = logs.iter().map(prepare_business_log).collect::<AuditResult<Vec<_>>>()?;
    db.audit_logs().create_many_ordered(&prepared, executor).await?;
    Ok(())
}

/// 在调用方事务中执行命令并记录一次结构化成功事件。
///
/// 静态身份和动作由 `BusinessEventContext::new` 在写入前验证；执行后投影在
/// 写审计前验证。本入口不启动事务、重试命令或写独立失败尝试记录。
///
/// # 参数
/// * `context` - 写入前已验证的动作、身份及关联快照。
/// * `sink` - 使用相同执行器保存事件的窄端口。
/// * `executor` - 调用方已建立的执行器。
/// * `command` - 拥有领域或命名 Process 的实际命令。
///
/// # 返回
/// 返回业务结果；领域明确的回放分支不追加业务事件。
///
/// # 错误
/// 业务失败、结果投影无效或审计写入失败时停止并返回原错误。
pub async fn execute_audited<C: AuditedCommand, S: AuditEventSink>(
    context: &BusinessEventContext,
    sink: &S,
    executor: &mut dyn Executor,
    command: &C,
) -> Result<C::Output> {
    match command.execute(executor).await? {
        AuditedWrite::Replayed(result) => Ok(result),
        AuditedWrite::Fresh { result, content } => {
            if content.result != BusinessEventResult::Succeeded {
                return Err(Error::ValidationError("成功业务事件不能记录拒绝或未知执行结果".to_string()));
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
    use erp_audit::{AuditAction, AuditFact, AuditValue};
    use erp_core::AccountKind;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    const ACTION: AuditAction = AuditAction {
        code: "test.confirm",
        resource_type: "test_record",
        label: "确认测试记录",
        version: 1,
        allowed_fields: &[],
    };

    struct TestExecutor;
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

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

    fn boundary(executor: &mut TestExecutor) -> RecordingBoundary {
        RecordingBoundary {
            identity: executor as *mut TestExecutor as usize,
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
        let mut executor = TestExecutor;
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
        let mut executor = TestExecutor;
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
            let mut executor = TestExecutor;
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
        let mut executor = TestExecutor;
        let mut boundary = boundary(&mut executor);
        boundary.invalid_content = true;
        assert!(execute_audited(&context(), &boundary, &mut executor, &boundary).await.is_err());
        assert_eq!(*boundary.calls.lock().unwrap(), ["execute"]);
    }

    #[tokio::test]
    async fn unknown_or_rejected_is_not_relabelled_as_success() {
        for result in [BusinessEventResult::Unknown, BusinessEventResult::Rejected] {
            let mut executor = TestExecutor;
            let mut boundary = boundary(&mut executor);
            boundary.result = result;
            assert!(execute_audited(&context(), &boundary, &mut executor, &boundary).await.is_err());
            assert_eq!(*boundary.calls.lock().unwrap(), ["execute"]);
        }
    }

    #[tokio::test]
    async fn unknown_business_outcome_preserves_original_source_and_never_reexecutes() {
        let mut executor = TestExecutor;
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
    use erp_audit::AuditActorLogs;
    use erp_core::AccountKind;

    use super::*;
    struct TestExecutor;
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Sink {
        identity: usize,
        fail: bool,
        calls: Arc<Mutex<Vec<&'static str>>>,
        events: Mutex<Vec<AuditLog>>,
    }
    #[async_trait]
    impl AuditEventSink for Sink {
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
            let mut executor = TestExecutor;
            let identity = &mut executor as *mut TestExecutor as usize;
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
            let result = execute_prepared(&audit, &sink, &mut executor, move |executor| {
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
            let mut executor = TestExecutor;
            let identity = &mut executor as *mut TestExecutor as usize;
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
}
