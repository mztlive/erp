//! HTTP 只等待领取结果；独立任务持有文件读取、识别及归档直至收尾。
use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use application_core::AuditActor;
use erp_contract::entity::recognition::{ImportStatus, ImportView};
use erp_data_import::contract_import::{ContractImportProcess, ImportAttempt};
use erp_data_import::{Error as ProcessError, Result as ProcessResult};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};
use tokio::time::timeout;

use crate::http::errors::Error;
use crate::http::rate_limit::Error as RateLimitError;
use crate::http::state::HttpState;
use crate::http::tasks::RequestTasks;

type Result<T> = std::result::Result<T, Error>;
static CAPACITY: OnceLock<Arc<Semaphore>> = OnceLock::new();

// 启动或重试合同识别时，把领取与识别交给有并发上限的后台任务，返回已领取或已成功的持久状态；掉线不取消识别。容量、权限、文件状态或领取失败时返回错误。
pub(crate) async fn start(state: HttpState, actor: AuditActor, id: String) -> Result<ImportView> {
    let permit = CAPACITY
        .get_or_init(|| Arc::new(Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::RateLimited(RateLimitError::ConcurrencyExceeded))?;
    let process = state.application().contract_import_process();
    let claiming = process.clone();
    let claimant = actor.clone();
    let task_id = id.clone();
    let tasks = state.tasks().clone();
    launch(
        &tasks,
        permit,
        async move { claiming.claim(&task_id, &claimant).await.map_err(Into::into) },
        move |attempt| execute(state, process, actor, id, attempt),
    )
    .await
}

#[tracing::instrument(name = "contract_import", skip_all, fields(
    account = actor.account(), request_id = actor.request_id(), task_id = %id
))]
async fn execute(
    state: HttpState,
    process: ContractImportProcess,
    actor: AuditActor,
    id: String,
    attempt: ImportAttempt,
) {
    let start = Instant::now();
    tracing::info!(event = "contract_import_started", "开始执行合同识别任务");
    let pdf = timeout(Duration::from_secs(30), async {
        let source = process.source(&id, &actor).await.map_err(|_| ())?;
        state.application().storage().read(&source.storage_object_key).await.map_err(|_| ())
    })
    .await;
    let result = match pdf {
        Ok(Ok(pdf)) => process.execute(attempt, &actor, &pdf).await,
        _ => process.source_failed(attempt).await,
    };
    log_outcome(result, start.elapsed());
}

fn log_outcome(result: ProcessResult<ImportView>, elapsed: Duration) {
    let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    match result {
        Ok(view) if matches!(view.status, ImportStatus::Failed) => tracing::warn!(
            event = "contract_import_finished",
            outcome = "failed",
            elapsed_ms,
            error_code = view.failure.as_ref().map(|failure| failure.code.as_str()),
            "合同识别任务失败，结果已保存"
        ),
        Ok(view) => tracing::info!(
            event = "contract_import_finished", status = ?view.status, elapsed_ms,
            "合同识别任务状态已保存"
        ),
        // 不输出原始错误、原文或供应商载荷；未知结果保留原任务用于查询。
        Err(error) => tracing::error!(
            event = "contract_import_finished",
            outcome = "unconfirmed",
            elapsed_ms,
            outcome_unknown = matches!(error, ProcessError::OutcomeUnknown(_)),
            "合同识别任务未确认完成，请通过原任务查询或恢复"
        ),
    }
}

// 在独立任务中领取合同识别工作并继续执行，响应接收端离开后仍完成已启动的步骤。
async fn launch<R, W, C, F, Run>(
    tasks: &RequestTasks,
    permit: OwnedSemaphorePermit,
    claim: C,
    run: Run,
) -> Result<R>
where
    R: Send + 'static,
    W: Send + 'static,
    C: Future<Output = Result<(R, Option<W>)>> + Send + 'static,
    Run: FnOnce(W) -> F + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    // 请求取消不取消工作；HTTP 进程退出时统一等待收尾。
    tasks.spawn(async move {
        let _permit = permit;
        match claim.await {
            Ok((view, work)) => {
                let _ = sender.send(Ok(view));
                if let Some(work) = work {
                    run(work).await;
                }
            },
            Err(error) => {
                let _ = sender.send(Err(error));
            },
        }
    })?;
    receiver.await.map_err(|_| Error::Internal("合同识别任务中断，请稍后查看原任务".into()))?
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::sync::Mutex;

    use erp_contract::entity::recognition::ImportFailure;
    use serde_json::Value;

    use super::*;

    #[derive(Default)]
    struct Capture(Mutex<Vec<u8>>);

    impl Write for &Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn persisted_business_failure_is_warn_and_never_logs_the_failure_message() {
        let view = ImportView {
            source_file_asset_id: "private-file-asset".into(),
            draft: None,
            expected_customer_id: None,
            revision_target: None,
            recoverable_at: None,
            id: "task-1".into(),
            version: 3,
            file_name: "private-file.pdf".into(),
            page_count: 4,
            status: ImportStatus::Failed,
            stage: None,
            started_at: None,
            extraction: None,
            result: None,
            failure: Some(ImportFailure::new("AI_TIMEOUT", "private-provider-message")),
            customer_id: None,
        };
        let buffer = Arc::new(Capture::default());
        let subscriber = tracing_subscriber::fmt().json().without_time().with_writer(buffer.clone()).finish();
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("contract_import", task_id = "task-1", request_id = "request-1");
            span.in_scope(|| log_outcome(Ok(view), Duration::from_secs(110)));
        });
        let bytes = buffer.0.lock().unwrap();
        let logs = String::from_utf8_lossy(&bytes);
        let event: Value = serde_json::from_str(logs.lines().next().unwrap()).unwrap();
        assert_eq!(event["level"], "WARN");
        assert_eq!(event["fields"]["event"], "contract_import_finished");
        assert_eq!(event["fields"]["error_code"], "AI_TIMEOUT");
        assert_eq!(event["fields"]["elapsed_ms"], 110_000);
        assert_eq!(event["span"]["task_id"], "task-1");
        assert_eq!(event["span"]["request_id"], "request-1");
        assert!(!logs.contains("private-provider-message"));
        assert!(!logs.contains("private-file.pdf"));
    }

    #[tokio::test]
    async fn cancelled_http_waiter_does_not_cancel_claim_or_completion() {
        let capacity = Arc::new(Semaphore::new(1));
        let (entered, entry) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let (finished, completion) = oneshot::channel();
        let permit = capacity.clone().try_acquire_owned().unwrap();
        let tasks = RequestTasks::default();
        let request_tasks = tasks.clone();
        let request = tokio::spawn(async move {
            launch(
                &request_tasks,
                permit,
                async move {
                    entered.send(()).unwrap();
                    released.await.unwrap();
                    Ok(("processing", Some(7)))
                },
                move |work| async move {
                    finished.send(work).unwrap();
                },
            )
            .await
        });
        entry.await.unwrap();
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        assert!(capacity.clone().try_acquire_owned().is_err());
        release.send(()).unwrap();
        assert_eq!(completion.await.unwrap(), 7);
        let _permit = capacity.acquire_owned().await.unwrap();
    }

    #[tokio::test]
    async fn acknowledges_before_work_finishes_and_holds_capacity_until_completion() {
        let capacity = Arc::new(Semaphore::new(1));
        let permit = capacity.clone().try_acquire_owned().unwrap();
        let (release, released) = oneshot::channel();
        let tasks = RequestTasks::default();
        let response = launch(&tasks, permit, async { Ok(("processing", Some(()))) }, move |_| async move {
            released.await.unwrap();
        })
        .await
        .unwrap();
        assert_eq!(response, "processing");
        assert!(capacity.clone().try_acquire_owned().is_err());
        release.send(()).unwrap();
        let _permit = capacity.acquire_owned().await.unwrap();
    }

    #[tokio::test]
    async fn replay_and_failed_claim_never_start_work_and_release_capacity() {
        for failure in [false, true] {
            let capacity = Arc::new(Semaphore::new(1));
            let permit = capacity.clone().try_acquire_owned().unwrap();
            let tasks = RequestTasks::default();
            let response = launch(
                &tasks,
                permit,
                async move {
                    if failure {
                        Err(Error::Conflict("任务已被领取".into()))
                    } else {
                        Ok(("succeeded", None::<()>))
                    }
                },
                |_| async { panic!("重放或失败不得执行工作") },
            )
            .await;
            assert_eq!(response.is_err(), failure);
            let _permit = capacity.acquire_owned().await.unwrap();
        }
    }
}
