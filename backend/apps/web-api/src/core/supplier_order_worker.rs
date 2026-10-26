//! 正式采购接口发单与供应商履约补查的有界后台调度。
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::app_state::AppState;

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
    let (stop, mut receiver) = watch::channel(false);
    let join = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                changed = receiver.changed() => {
                    if changed.is_err() || *receiver.borrow() {
                        break;
                    }
                },
                _ = interval.tick() => {
                    if let Some(process) = state.supplier_api_order_process()
                        && let Err(error) = process.run_once().await
                    {
                        tracing::warn!(error = %error, "供应商自动发单调度失败");
                    }
                    if let Some(process) = state.supplier_order_tracking_process()
                        && let Err(error) = process.run_once().await
                    {
                        tracing::warn!(error = %error, "供应商履约跟踪调度失败");
                    }
                },
            }
        }
    });
    SupplierOrderWorker { stop, join }
}
