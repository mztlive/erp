//! 应用组合根：为文件任务装配原业务查询，不保存 HTTP token。
mod reads;
use std::sync::Arc;
use std::time::Duration;

use background_runtime::periodic;
use erp_data_import::data_transfer::{BusinessTransferServices, BusinessTransfers, DataTransferProcess};
use erp_supplier_catalog::supplier_portal::SupplierPortalProcess;
use tokio::sync::watch;

use crate::app_state::AppState;
use crate::wiring::{catalog_services, scoped_offering_process};

pub(crate) fn process(state: &AppState) -> DataTransferProcess {
    let reader = Arc::new(reads::Reads(state.clone()));
    let services = BusinessTransferServices {
        inquiry: state.inquiry_import_process(),
        offerings: Arc::new(scoped_offering_process(state.db(), state.rbac())),
        portal: Arc::new(SupplierPortalProcess::new(
            state.db(),
            state.rbac(),
            catalog_services(state.db(), state.rbac()),
        )),
    };
    let handler =
        Arc::new(BusinessTransfers::new(state.db(), state.rbac(), reader, state.storage().clone(), services));
    DataTransferProcess::new(state.db(), state.storage().clone(), state.sensitive_data(), handler)
}
/// 执行持久化导入导出，停止后等待当前轮次结束。
/// # 参数
/// `state` 为应用装配，`stop` 为停止通知。
/// # 返回
/// 当前轮次已完成，不再领取任务。
/// # 错误
/// 单轮失败记录日志，沿原持久化状态恢复。
pub async fn run(state: AppState, stop: watch::Receiver<bool>) {
    let process = process(&state);
    periodic(stop, Duration::from_secs(2), || async {
        if let Err(error) = process.run_due().await {
            tracing::warn!(%error, "导入导出任务轮询失败，稍后重试");
        }
    })
    .await;
}
