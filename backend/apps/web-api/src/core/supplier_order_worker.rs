//! 正式采购接口发单与供应商履约补查的有界后台调度。
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::app_state::{AppState, JdVopCredentialMaintenance};

/// 进程退出时停止领取新的供应商订单任务。
pub struct SupplierOrderWorker {
    stop: watch::Sender<bool>,
    join: JoinHandle<()>,
}

impl SupplierOrderWorker {
    /// 停止调度并等待当前有界协议请求结束。
    /// # 参数
    /// 无；消耗本worker句柄。
    /// # 返回
    /// 调度任务已退出。
    /// # 错误
    /// 任务取消只记入运行日志，不掩盖进程退出。
    pub async fn stop(self) {
        let _ = self.stop.send(true);
        if let Err(error) = self.join.await {
            tracing::warn!(error=%error,"供应商订单调度退出异常");
        }
    }
}

/// 启动正式订单发单与只读跟踪；未配置适配器的部署不执行外部请求。
/// # 参数
/// `state` 为已装配真实协议客户端和加密器的应用状态。
/// # 返回
/// 可用于优雅退出的任务句柄。
/// # 错误
/// 不返回错误；单轮失败记录日志并继续后续调度。
pub fn spawn(state: AppState) -> SupplierOrderWorker {
    let (stop, receiver) = watch::channel(false);
    let join = tokio::spawn(async move {
        tokio::join!(
            execution_workers(state.clone(), receiver.clone()),
            integration_workers(state, receiver),
        );
    });
    SupplierOrderWorker { stop, join }
}

async fn execution_workers(state: AppState, receiver: watch::Receiver<bool>) {
    let order_process = state.supplier_api_order_process();
    let tracking_process = state.supplier_order_tracking_process();
    tokio::join!(
        periodic(receiver.clone(), || async {
            if let Some(process) = &order_process
                && let Err(error) = process.run_once().await
            {
                tracing::warn!(error = %error, "供应商自动发单调度失败");
            }
        }),
        periodic(receiver, || async {
            if let Some(process) = &tracking_process
                && let Err(error) = process.run_once().await
            {
                tracing::warn!(error = %error, "供应商履约跟踪调度失败");
            }
        }),
    );
}

async fn integration_workers(state: AppState, receiver: watch::Receiver<bool>) {
    let catalog_callbacks = state.supplier_catalog_callback_process();
    let funds_worker = state.supplier_funds_worker();
    let credentials = state.jd_vop_credential_maintenance();
    let message_worker = state.supplier_message_worker().map(|worker| {
        if let Some(funds) = &funds_worker { worker.with_funds_investigation(funds.clone()) } else { worker }
    });
    // 各自轮询；订单、消息和凭据维护不会互相占用请求等待时间。
    tokio::join!(
        periodic(receiver.clone(), || async {
            if let Some(worker) = &funds_worker
                && let Err(error) = worker.run_once().await
            {
                tracing::warn!(error = %error, "供应商资金调查调度失败");
            }
        }),
        periodic(receiver.clone(), || async {
            if let Some(worker) = &message_worker
                && let Err(error) = worker.run_once().await
            {
                tracing::warn!(error = %error, "供应商消息补查调度失败");
            }
        }),
        periodic(receiver.clone(), || async {
            if let Some(process) = &catalog_callbacks
                && let Err(error) = process.run_once().await
            {
                tracing::warn!(error = %error, "供应商商品回调调度失败");
            }
        }),
        credential_worker(receiver, credentials),
    );
}

async fn credential_worker(
    receiver: watch::Receiver<bool>,
    credentials: Option<Arc<JdVopCredentialMaintenance>>,
) {
    let Some(credentials) = credentials else {
        return;
    };
    periodic_at(receiver, Duration::from_secs(60), || async {
        if let Err(error) = credentials.run_once().await {
            tracing::warn!(code = %error.code, "京东账号授权维护受阻，请核对原续期状态");
        }
    })
    .await;
    if let Err(error) = credentials.settle_refresh().await {
        tracing::warn!(code = %error.code, "京东原续期尚未完成保存，请保留原凭据状态继续核对");
    }
}

async fn periodic<F, Fut>(receiver: watch::Receiver<bool>, run: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = ()>,
{
    periodic_at(receiver, Duration::from_secs(5), run).await;
}

async fn periodic_at<F, Fut>(mut receiver: watch::Receiver<bool>, period: Duration, run: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = ()>,
{
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            changed = receiver.changed() => {
                if changed.is_err() || *receiver.borrow() { break; }
            },
            _ = interval.tick() => {
                if *receiver.borrow() { break; }
                run().await;
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::ready;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn stopping_or_dropping_scheduler_never_claims_new_work() {
        for already_stopped in [false, true] {
            let (sender, receiver) = watch::channel(already_stopped);
            if !already_stopped {
                drop(sender);
            }
            let calls = Arc::new(AtomicUsize::new(0));
            let observed = calls.clone();
            periodic(receiver, move || {
                observed.fetch_add(1, Ordering::SeqCst);
                ready(())
            })
            .await;
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn credential_periodic_stops_after_current_check_and_does_not_start_another() {
        let (sender, receiver) = watch::channel(false);
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        tokio::time::timeout(
            Duration::from_secs(1),
            periodic_at(receiver, Duration::from_secs(60), move || {
                observed.fetch_add(1, Ordering::SeqCst);
                sender.send(true).unwrap();
                ready(())
            }),
        )
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
