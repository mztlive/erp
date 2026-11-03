//! 请求衍生工作在请求取消后继续执行，由进程统一排空。
use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::task::{JoinError, JoinSet};

use super::errors::Error;

#[derive(Default)]
struct Tasks {
    closing: bool,
    running: JoinSet<()>,
}

/// HTTP 进程共享的请求衍生任务集合。
#[derive(Clone, Default)]
pub(crate) struct RequestTasks(Arc<Mutex<Tasks>>);
impl RequestTasks {
    /// 登记独立于请求等待方的工作。
    /// # 参数
    /// work 为请求取消后仍须收尾的工作。
    /// # 返回
    /// 任务已交由进程管理。
    /// # 错误
    /// 停机或管理锁不可用时拒绝登记。
    pub(crate) fn spawn(&self, work: impl Future<Output = ()> + Send + 'static) -> Result<(), Error> {
        let mut tasks = self.0.lock().map_err(|_| Error::Internal("请求任务管理不可用".into()))?;
        if tasks.closing {
            return Err(Error::Internal("服务正在停止，请查询原任务状态".into()));
        }
        while let Some(result) = tasks.running.try_join_next() {
            log_result(result);
        }
        tasks.running.spawn(work);
        Ok(())
    }
    /// 停止准入并排空已开始的工作。
    /// # 参数
    /// 无；HTTP 请求排空后调用，总超时由进程监督器控制。
    /// # 返回
    /// 已登记任务全部完成或记录异常。
    /// # 错误
    /// 管理锁中毒时恢复其中的任务集合继续收尾。
    pub(crate) async fn drain(&self) {
        let mut running = {
            let mut tasks = self.0.lock().unwrap_or_else(|error| error.into_inner());
            tasks.closing = true;
            std::mem::take(&mut tasks.running)
        };
        while let Some(result) = running.join_next().await {
            log_result(result);
        }
    }
}
fn log_result(result: Result<(), JoinError>) {
    if let Err(error) = result {
        tracing::error!(%error, "请求衍生任务异常退出，请核对原任务状态");
    }
}
#[cfg(test)]
mod tests {
    use tokio::sync::oneshot;

    use super::*;
    #[tokio::test]
    async fn drain_waits_for_completion_and_rejects_new_work() {
        let tasks = RequestTasks::default();
        let (release, released) = oneshot::channel();
        let (done, completed) = oneshot::channel();
        tasks
            .spawn(async move {
                released.await.unwrap();
                done.send(()).unwrap();
            })
            .unwrap();
        let drain = tasks.drain();
        tokio::pin!(drain);
        assert!(tokio::time::timeout(std::time::Duration::from_millis(10), &mut drain).await.is_err());
        assert!(tasks.spawn(async { panic!("closed task must not run") }).is_err());
        release.send(()).unwrap();
        drain.await;
        completed.await.unwrap();
    }
    #[tokio::test]
    async fn failed_task_does_not_cancel_other_registered_work() {
        let tasks = RequestTasks::default();
        let (done, completed) = oneshot::channel();
        tasks.spawn(async { panic!("request task panic fixture") }).unwrap();
        tasks
            .spawn(async move {
                done.send(()).unwrap();
            })
            .unwrap();
        tasks.drain().await;
        completed.await.unwrap();
        assert!(tasks.spawn(async {}).is_err());
    }
}
