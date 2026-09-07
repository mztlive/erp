//! 实际执行策略：启动事务返回后调用网关，再提交结果事务。

use std::future::Future;

use application_core::AuditActor;
use services::Result;

/// 启动/结束方法拥有各自根事务；网关方法只消费已提交的事实，不接 Executor。
pub(super) trait ConnectionJobExecutionPort: Sync {
    type Job: Send;
    type Started: Send + Sync;
    type Outcome: Send;

    fn start(&self, job: Self::Job) -> impl Future<Output = Result<Self::Started>> + Send;
    fn invoke(&self, started: &Self::Started) -> impl Future<Output = Self::Outcome> + Send;
    fn finish(
        &self,
        started: Self::Started,
        outcome: Self::Outcome,
        actor: &AuditActor,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// 健康检查和目录同步共同消费的生产轨迹；分类网关失败仍交结果事务登记。
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
    use services::Error;

    use super::*;

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
        RecordingExecution {
            trace: Mutex::new(Trace::default()),
            fail_at,
            outcome,
        }
    }

    #[tokio::test]
    async fn gateway_is_between_committed_start_and_new_result_transaction() {
        let port = port(None, Ok(()));
        execute(&port, 41, &actor()).await.unwrap();
        let trace = port.trace.lock().unwrap();
        assert_eq!(
            trace.calls,
            [
                "start.begin",
                "start.commit",
                "gateway",
                "finish.begin",
                "finish.commit"
            ]
        );
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
        assert_eq!(
            trace.calls,
            [
                "start.begin",
                "start.commit",
                "gateway",
                "finish.begin",
                "finish.commit"
            ]
        );
    }

    #[tokio::test]
    async fn execution_preserves_first_transaction_error_without_later_steps() {
        for (failure, expected) in [
            ("start", vec!["start.begin", "start.abort"]),
            (
                "finish",
                vec![
                    "start.begin",
                    "start.commit",
                    "gateway",
                    "finish.begin",
                    "finish.abort",
                ],
            ),
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
