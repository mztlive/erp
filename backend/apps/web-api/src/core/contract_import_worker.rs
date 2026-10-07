//! HTTP 只等待领取结果；独立任务持有文件读取、识别及归档直至收尾。
use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use application_core::AuditActor;
use erp_contract::entity::recognition::ImportView;
use erp_processes::Error as ProcessError;
use erp_processes::contract_import::{ContractImportProcess, ImportAttempt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};
use tokio::time::timeout;

use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::rate_limit::Error as RateLimitError;

type Result<T> = std::result::Result<T, Error>;
static CAPACITY: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// 启动有界后台工作，立即返回已领取或已成功的任务。
/// # 参数
/// * `state` / `actor` / `id` - 应用、认证人与本人任务。
/// # 返回
/// 持久任务状态；响应被取消后后台仍继续执行。
/// # 错误
/// 并发容量不足、权限、文件状态或任务领取失败。
pub(crate) async fn start(state: AppState, actor: AuditActor, id: String) -> Result<ImportView> {
    let permit = CAPACITY
        .get_or_init(|| Arc::new(Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::RateLimited(RateLimitError::ConcurrencyExceeded))?;
    let process = state.contract_import_process();
    let claiming = process.clone();
    let claimant = actor.clone();
    let task_id = id.clone();
    launch(
        permit,
        async move { claiming.claim(&task_id, &claimant).await.map_err(Into::into) },
        move |attempt| execute(state, process, actor, id, attempt),
    )
    .await
}

async fn execute(
    state: AppState,
    process: ContractImportProcess,
    actor: AuditActor,
    id: String,
    attempt: ImportAttempt,
) {
    let pdf = timeout(Duration::from_secs(30), async {
        let source = process.source(&id, &actor).await.map_err(|_| ())?;
        state.storage().read(&source.storage_object_key).await.map_err(|_| ())
    })
    .await;
    let result = match pdf {
        Ok(Ok(pdf)) => process.execute(attempt, &actor, &pdf).await,
        _ => process.source_failed(attempt).await,
    };
    if let Err(error) = result {
        // 不输出原始错误、原文或供应商载荷；未知结果保留原任务用于查询。
        tracing::error!(
            account = actor.account(),
            request_id = actor.request_id(),
            task_id = id,
            outcome_unknown = matches!(error, ProcessError::OutcomeUnknown(_)),
            "合同识别任务未确认完成，请通过原任务查询或恢复"
        );
    }
}

/// 领取和工作都归独立任务所有，丢弃响应接收端不得取消任何已启动步骤。
async fn launch<R, W, C, F, Run>(permit: OwnedSemaphorePermit, claim: C, run: Run) -> Result<R>
where
    R: Send + 'static,
    W: Send + 'static,
    C: Future<Output = Result<(R, Option<W>)>> + Send + 'static,
    Run: FnOnce(W) -> F + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    // 明确 detach：工作自行持久化结果并记录收尾失败；进程重启沿原任务恢复。
    drop(tokio::spawn(async move {
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
    }));
    receiver.await.map_err(|_| Error::Internal("合同识别任务中断，请稍后查看原任务".into()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_http_waiter_does_not_cancel_claim_or_completion() {
        let capacity = Arc::new(Semaphore::new(1));
        let (entered, entry) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let (finished, completion) = oneshot::channel();
        let permit = capacity.clone().try_acquire_owned().unwrap();
        let request = tokio::spawn(launch(
            permit,
            async move {
                entered.send(()).unwrap();
                released.await.unwrap();
                Ok(("processing", Some(7)))
            },
            move |work| async move {
                finished.send(work).unwrap();
            },
        ));
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
        let response = launch(permit, async { Ok(("processing", Some(()))) }, move |_| async move {
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
            let response = launch(
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
