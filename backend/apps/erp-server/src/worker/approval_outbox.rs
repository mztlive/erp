//! 审批 outbox 的应用进程生命周期。
use std::sync::Arc;
use std::time::Duration;

use erp_workflow::ApprovalNotificationOutboxPort;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

const OUTBOX_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// 审批通知 outbox worker 句柄。停止时不再领取新租约。
pub struct ApprovalOutboxWorker {
    /// 置位后 worker 停止领取新租约。
    stop_tx: watch::Sender<bool>,
    /// 后台轮询任务。
    join: JoinHandle<()>,
}

impl ApprovalOutboxWorker {
    // 启动通知轮询，未接入提供方时等待停止信号。
    // `port` 为通知执行端口。
    // 可停止和监督的循环句柄。
    pub(crate) fn start(port: Arc<ApprovalNotificationOutboxPort>) -> Self {
        let (stop_tx, stop_rx) = watch::channel(false);
        let worker_id = format!("erp-background-{}", id_generator::next_id());
        let join = tokio::spawn(run_approval_outbox_worker(port, worker_id, stop_rx));
        Self { stop_tx, join }
    }

    // 查询常驻循环是否意外结束。
    // 后台句柄已经完成时为 true。
    pub(crate) fn is_finished(&self) -> bool {
        self.join.is_finished()
    }

    // 通知停止领取；已运行轮次由 stop 等待。
    // 已发送停止状态。
    // 接收端已退出时无需再次通知。
    pub(crate) fn request_stop(&self) {
        let _ = self.stop_tx.send(true);
    }

    /// 停止领取新租约并等待当前批次结束。
    ///
    /// 进程被强制终止时同样不再领取；未完成租约会到期后由其他实例接管。
    /// 当前轮次已结束。
    /// 循环异常记录日志，不重放业务写入。
    pub async fn stop(self) {
        if self.stop_tx.send(true).is_err() {
            info!("审批通知 outbox worker 已退出");
        }
        if let Err(error) = self.join.await {
            error!(error = %error, "等待审批通知 outbox worker 结束失败");
        }
    }
}

// 运行 outbox worker，直到收到停止信号。
// `port` 为工作流领域 outbox 端口。
async fn run_approval_outbox_worker(
    port: Arc<ApprovalNotificationOutboxPort>,
    worker_id: String,
    mut stop_rx: watch::Receiver<bool>,
) {
    if !port.is_ready() {
        warn!("审批通知提供方未接入，投递 worker 已暂停；待投递记录保留，不消耗重试次数");
        wait_for_stop(stop_rx).await;
        return;
    }
    info!(worker_id = %worker_id, "审批通知 outbox worker 已启动");
    loop {
        if *stop_rx.borrow() {
            info!(worker_id = %worker_id, "审批通知 outbox worker 停止领取新租约");
            return;
        }
        if let Err(error) = port.process_tick(&worker_id).await {
            error!(worker_id = %worker_id, error = %error, "审批通知 outbox 批次处理失败");
        }
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    info!(worker_id = %worker_id, "审批通知 outbox worker 已停止");
                    return;
                }
            }
            () = tokio::time::sleep(OUTBOX_POLL_INTERVAL) => {}
        }
    }
}

// 通知提供方未装配时不领取 outbox；等待进程请求停机或监督器关闭信号通道。
async fn wait_for_stop(mut stop: watch::Receiver<bool>) {
    while !*stop.borrow() {
        if stop.changed().await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn paused_worker_remains_alive_until_stop_or_owner_drop() {
        let (send, receive) = watch::channel(false);
        let mut task = tokio::spawn(wait_for_stop(receive));
        assert!(tokio::time::timeout(Duration::from_millis(10), &mut task).await.is_err());
        send.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(1), task).await.unwrap().unwrap();
        let (send, receive) = watch::channel(false);
        drop(send);
        tokio::time::timeout(Duration::from_secs(1), wait_for_stop(receive)).await.unwrap();
    }
}
