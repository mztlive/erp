//! 实际执行策略：启动事务返回后调用网关，再提交结果事务。

use std::future::Future;

use application_core::AuditActor;

use crate::Result;

/// 启动/结束方法拥有各自根事务；网关方法只消费已提交的事实，不接 Executor。
pub(super) trait ConnectionJobExecutionPort: Sync {
    type Job: Send;
    type Started: Send + Sync;
    type Outcome: Send;

    /// 在独立根事务中启动任务，返回已提交、可供网关使用的事实。
    ///
    /// # 参数
    /// * `job` - 待启动的任务。
    ///
    /// # 返回
    /// 返回实现方定义的已提交启动结果。
    ///
    /// # 错误
    /// 启动事务失败时返回错误，调用方不得继续调用网关。
    fn start(&self, job: Self::Job) -> impl Future<Output = Result<Self::Started>> + Send;

    /// 在事务外调用网关。分类失败放进 `Outcome`，不得在这里中断收尾。
    ///
    /// # 参数
    /// * `started` - `start` 返回的已提交事实。
    ///
    /// # 返回
    /// 返回实现方定义的网关结果，包括已分类失败。
    ///
    /// # 错误
    /// 不返回错误。
    fn invoke(&self, started: &Self::Started) -> impl Future<Output = Self::Outcome> + Send;

    /// 在独立结果事务中登记网关结果。
    ///
    /// # 参数
    /// * `started` - `start` 返回的已提交事实。
    /// * `outcome` - `invoke` 返回的网关结果。
    /// * `actor` - 结果事务使用的审计操作人。
    ///
    /// # 返回
    /// 结果事务提交成功时返回。
    ///
    /// # 错误
    /// 结果落库失败时返回错误。
    fn finish(
        &self,
        started: Self::Started,
        outcome: Self::Outcome,
        actor: &AuditActor,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// 先启动、再调用网关、最后登记结果；网关失败仍进入 `finish`。
///
/// # 参数
/// * `port` - 启动、调用与收尾实现。
/// * `job` - 待执行任务。
/// * `actor` - 结果事务使用的审计操作人。
///
/// # 返回
/// 启动与收尾都成功时返回。
///
/// # 错误
/// `start` 或 `finish` 失败时返回对应错误。`invoke` 的失败不在此处短路。
pub(super) async fn execute<P: ConnectionJobExecutionPort>(
    port: &P,
    job: P::Job,
    actor: &AuditActor,
) -> Result<()> {
    let started = port.start(job).await?;
    let outcome = port.invoke(&started).await;
    port.finish(started, outcome, actor).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use erp_supply::entity::failure::SupplierFailureClass;
    use erp_supply::ports::supplier_api_gateway::ClassifiedError;

    use super::*;
    use crate::Error;

    #[derive(Default)]
    struct Trace {
        transaction_active: bool,
        calls: Vec<&'static str>,
        settled: Option<std::result::Result<(), ClassifiedError>>,
    }

    struct RecordingExecution {
        trace: Mutex<Trace>,
        fail_at: Option<&'static str>,
        outcome: std::result::Result<(), ClassifiedError>,
    }

    impl ConnectionJobExecutionPort for RecordingExecution {
        type Job = u32;
        type Started = u32;
        type Outcome = std::result::Result<(), ClassifiedError>;

        async fn start(&self, job: Self::Job) -> Result<Self::Started> {
            let mut trace = self.trace.lock().unwrap();
            assert!(!trace.transaction_active);
            trace.transaction_active = true;
            trace.calls.push("start.begin");
            if self.fail_at == Some("start") {
                trace.transaction_active = false;
                trace.calls.push("start.abort");
                return Err(Error::ConflictError("start rejected".into()));
            }
            trace.calls.push("start.commit");
            trace.transaction_active = false;
            Ok(job + 1)
        }

        async fn invoke(&self, started: &Self::Started) -> Self::Outcome {
            let mut trace = self.trace.lock().unwrap();
            assert_eq!(*started, 42);
            assert!(!trace.transaction_active, "网关不能持有启动或结果事务");
            assert_eq!(trace.calls.last(), Some(&"start.commit"));
            trace.calls.push("gateway");
            self.outcome.clone()
        }

        async fn finish(
            &self,
            started: Self::Started,
            outcome: Self::Outcome,
            actor: &AuditActor,
        ) -> Result<()> {
            let mut trace = self.trace.lock().unwrap();
            assert_eq!(started, 42);
            assert_eq!(actor.id(), "actor-1");
            assert!(!trace.transaction_active);
            assert_eq!(trace.calls.last(), Some(&"gateway"));
            trace.transaction_active = true;
            trace.calls.push("finish.begin");
            trace.settled = Some(outcome);
            if self.fail_at == Some("finish") {
                trace.transaction_active = false;
                trace.calls.push("finish.abort");
                return Err(Error::ConflictError("finish rejected".into()));
            }
            trace.calls.push("finish.commit");
            trace.transaction_active = false;
            Ok(())
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("actor-1".into(), "actor".into(), AccountKind::Admin)
    }

    fn port(
        fail_at: Option<&'static str>,
        outcome: std::result::Result<(), ClassifiedError>,
    ) -> RecordingExecution {
        RecordingExecution { trace: Mutex::new(Trace::default()), fail_at, outcome }
    }

    #[tokio::test]
    async fn gateway_is_between_committed_start_and_new_result_transaction() {
        let port = port(None, Ok(()));
        execute(&port, 41, &actor()).await.unwrap();
        let trace = port.trace.lock().unwrap();
        assert_eq!(trace.calls, ["start.begin", "start.commit", "gateway", "finish.begin", "finish.commit"]);
        assert_eq!(trace.settled, Some(Ok(())));
        assert!(!trace.transaction_active);
    }

    #[tokio::test]
    async fn classified_gateway_failure_still_reaches_result_transaction_unchanged() {
        let failure = ClassifiedError {
            class: SupplierFailureClass::ResultUnknown,
            code: "OUTCOME_UNKNOWN".into(),
            summary: "结果未知".into(),
        };
        let port = port(None, Err(failure.clone()));
        execute(&port, 41, &actor()).await.unwrap();
        let trace = port.trace.lock().unwrap();
        assert_eq!(trace.settled, Some(Err(failure)));
        assert_eq!(trace.calls, ["start.begin", "start.commit", "gateway", "finish.begin", "finish.commit"]);
    }

    #[tokio::test]
    async fn execution_preserves_first_transaction_error_without_later_steps() {
        for (failure, expected) in [
            ("start", vec!["start.begin", "start.abort"]),
            ("finish", vec!["start.begin", "start.commit", "gateway", "finish.begin", "finish.abort"]),
        ] {
            let port = port(Some(failure), Ok(()));
            let error = execute(&port, 41, &actor()).await.unwrap_err();
            assert!(
                matches!(error, Error::ConflictError(message) if message == format!("{failure} rejected"))
            );
            let trace = port.trace.lock().unwrap();
            assert_eq!(trace.calls, expected);
            assert!(!trace.transaction_active);
        }
    }
}
