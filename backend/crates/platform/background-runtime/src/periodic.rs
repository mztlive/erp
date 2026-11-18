//! 可停止的轮询：只在轮次边界退出，不丢弃正在执行的业务 future。
use std::future::Future;
use std::time::Duration;

use tokio::sync::watch;

pub(super) fn should_stop(stop: &watch::Receiver<bool>) -> bool {
    *stop.borrow() || stop.has_changed().is_err()
}

/// 周期执行工作；发送停止信号或释放所有发送端后结束。
/// # 参数
/// `stop` 为停止通知，`period` 为轮次间等待时间，`run` 为一个有界业务轮次。
/// # 返回
/// 已停止领取且当前轮次执行完毕。
/// # 错误
/// 不处理业务错误，由 `run` 自行记录；panic 交由进程监督器处理。
pub async fn periodic<F, Fut>(mut stop: watch::Receiver<bool>, period: Duration, mut run: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()>,
{
    loop {
        if should_stop(&stop) {
            return;
        }
        run().await;
        tokio::select! {
            biased;
            _ = stop.changed() => {},
            _ = tokio::time::sleep(period) => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::Notify;

    use super::*;

    #[tokio::test]
    async fn stop_waits_for_current_work_and_prevents_next_round() {
        let (send, receive) = watch::channel(false);
        let started = Arc::new(Notify::new());
        let finish = Arc::new(Notify::new());
        let count = Arc::new(AtomicUsize::new(0));
        let (started_task, finish_task, count_task) = (started.clone(), finish.clone(), count.clone());
        let task = tokio::spawn(async move {
            periodic(receive, Duration::ZERO, || async {
                count_task.fetch_add(1, Ordering::SeqCst);
                started_task.notify_one();
                finish_task.notified().await;
            })
            .await;
        });
        started.notified().await;
        send.send(true).unwrap();
        assert!(!task.is_finished());
        finish.notify_one();
        tokio::time::timeout(Duration::from_secs(1), task).await.unwrap().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn already_stopped_or_dropped_sender_never_runs() {
        let (send, receive) = watch::channel(true);
        periodic(receive, Duration::ZERO, || async { panic!("must not run") }).await;
        let (send2, receive2) = watch::channel(false);
        drop((send, send2));
        periodic(receive2, Duration::ZERO, || async { panic!("must not run") }).await;
    }
}
