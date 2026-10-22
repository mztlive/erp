//! 应用组合根：为文件任务装配原业务查询，不保存 HTTP token。
mod reads;
use std::sync::Arc;
use std::time::Duration;

use erp_processes::data_transfer::{BusinessTransfers, DataTransferProcess};
use tokio::time::sleep;

use crate::app_state::AppState;

pub(crate) fn process(state: &AppState) -> DataTransferProcess {
    let reader = Arc::new(reads::Reads(state.clone()));
    let handler = Arc::new(BusinessTransfers::new(state.db(), state.rbac(), reader, state.storage().clone()));
    DataTransferProcess::new(state.db(), state.storage().clone(), state.sensitive_data(), handler)
}
/// 启动持久化导入导出执行循环。
/// # 参数
/// 已完成存储与权限装配的应用状态。
/// # 返回
/// 立即返回，执行循环随进程运行。
/// # 错误
/// 单轮失败写入日志，下一轮继续恢复。
pub fn spawn(state: AppState) {
    let process = process(&state);
    // 明确 detach；任务及输入已持久化，退出后由下一实例重新认领。
    drop(tokio::spawn(async move {
        loop {
            if process.run_due().await.is_err() {
                tracing::warn!("导入导出任务轮询失败，稍后重试");
            }
            sleep(Duration::from_secs(2)).await;
        }
    }));
}
