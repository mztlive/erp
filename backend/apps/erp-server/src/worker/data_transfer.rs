//! 持久化导入导出消费循环。
use std::time::Duration;

use background_runtime::periodic;
use tokio::sync::watch;

use crate::wiring::Application;
use crate::wiring::data_transfer::process;
// 执行持久化导入导出，停止后等待当前轮次结束。
// `state` 为应用装配，`stop` 为停止通知。
// 当前轮次已完成，不再领取任务。
// 单轮失败记录日志，沿原持久化状态恢复。
pub(super) async fn run(state: Application, stop: watch::Receiver<bool>) {
    let process = process(&state);
    periodic(stop, Duration::from_secs(2), || async {
        if let Err(error) = process.run_due().await {
            tracing::warn!(%error, "导入导出任务轮询失败，稍后重试");
        }
    })
    .await;
}
