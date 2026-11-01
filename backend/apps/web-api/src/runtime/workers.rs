//! 常驻任务装配与监督；所有循环共享进程退出边界。
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use background_runtime::BackgroundRunner;
use erp_data_import::background::{ProductImportTaskAdapter, SupplierImportTaskAdapter};
use erp_order_to_cash::background::SalesSelectionTaskAdapter;
use erp_order_to_cash::sales_selection::SalesSelectionProcess;
use erp_supplier_catalog::background::{
    DeliveryCoverageRefreshAdapter, SellableProjectionRefreshAdapter, SupplierCatalogAdoptionTaskAdapter,
    SupplierCatalogTaskAdapter,
};
use erp_supplier_catalog::delivery_coverage_refresh::DeliveryCoverageRefreshProcess;
use erp_supplier_catalog::sellable_projection_refresh::SellableProjectionRefresher;
use erp_supplier_catalog::source_media::SourceMediaWorker;
use erp_supplier_catalog::supplier_connection_execution::CatalogTaskControl;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::Result;
use super::xxl::XxlExecutor;
use crate::app_state::{AppState, ApprovalOutboxWorker};
use crate::core::data_transfer;
use crate::core::supplier_order_worker::{self, SupplierOrderWorker};

/// 进程唯一的常驻执行器集合。
pub(super) struct Workers {
    stop: watch::Sender<bool>,
    xxl: Arc<XxlExecutor>,
    tasks: Vec<JoinHandle<()>>,
    catalog: CatalogTaskControl,
    orders: SupplierOrderWorker,
    outbox: ApprovalOutboxWorker,
}

impl Workers {
    /// 先完成所有可失败装配，再启动任何循环。
    /// # 参数
    /// state 为已装配的应用组合根。
    /// # 返回
    /// 进程唯一的常驻执行器集合。
    /// # 错误
    /// 依赖装配或旧调度回执恢复失败时拒绝启动。
    pub(super) async fn start(state: AppState) -> Result<Self> {
        let config = state.config_snapshot().xxl_job;
        let catalog = CatalogTaskControl::default();
        let source_media = state.supplier_source_media_preparer()?;
        let runner = build_runner(&state)?;
        let execution = state.supplier_connection_execution_process().with_source_media(source_media.clone());
        let catalog_adapter = Arc::new(SupplierCatalogTaskAdapter::new(
            state.db(),
            execution,
            state.supplier_api_governance_process(),
            catalog.clone(),
        ));
        let runner = runner.register_shared(catalog_adapter.clone());
        let xxl = XxlExecutor::new(&state, config, catalog_adapter).await?;
        tracing::info!("Worker 定时入口统一由 XXL-JOB 驱动");
        let (stop, receive) = watch::channel(false);
        let mut tasks = xxl.spawn(receive.clone());
        if let Some(preparer) = source_media {
            let media = SourceMediaWorker::new(
                state.db(),
                preparer,
                crate::wiring::scoped_catalog_service(state.db(), state.rbac()),
            );
            let receive = receive.clone();
            tasks.push(tokio::spawn(async move {
                media.run(receive).await;
            }));
        }
        let runner_stop = receive.clone();
        tasks.push(tokio::spawn(async move {
            runner.run(runner_stop).await;
        }));
        tasks.push(tokio::spawn(data_transfer::run(state.clone(), receive)));
        let orders = supplier_order_worker::spawn(state.clone());
        let outbox = state.start_approval_outbox_worker();
        Ok(Self { stop, tasks, catalog, orders, outbox, xxl })
    }

    /// 独立 Worker 必备的 XXL 协议路由。
    /// # 参数
    /// 无。
    /// # 返回
    /// 执行器协议路由。
    /// # 错误
    /// 无。
    pub(super) fn scheduler_routes(&self) -> Router {
        self.xxl.router()
    }

    /// 等待任一常驻循环意外退出；正常业务错误仍由各循环处理。
    pub(super) async fn failed(&self) {
        wait_for_failure(&self.tasks, || self.orders.is_finished() || self.outbox.is_finished()).await;
    }

    /// 同时停止所有入口，防止串行排空期间其他循环继续领取。
    pub(super) fn request_stop(&self) {
        self.xxl.request_stop();
        self.stop.send_replace(true);
        self.catalog.request_stop();
        self.orders.request_stop();
        self.outbox.request_stop();
    }

    /// 排空已开始的工作；总超时由进程统一控制。
    pub(super) async fn drain(self) {
        let joins = async {
            for task in self.tasks {
                if let Err(error) = task.await {
                    tracing::error!(%error, "后台循环退出异常");
                }
            }
        };
        tokio::join!(joins, self.orders.stop(), self.outbox.stop());
    }
}

/// 装配同步和异步适配器，不改变原有业务认领顺序。
fn build_runner(state: &AppState) -> Result<BackgroundRunner> {
    Ok(BackgroundRunner::new(Duration::from_secs(2))
        .register(ProductImportTaskAdapter::new(state.product_import_process()))
        .register(SupplierImportTaskAdapter::new(state.supplier_import_process()))
        .register(SupplierCatalogAdoptionTaskAdapter::new(
            state.db(),
            state.supplier_catalog_adoption_process()?,
        ))
        .register(SalesSelectionTaskAdapter::new(SalesSelectionProcess::new(
            state.db(),
            state.storage().clone(),
            state.config_snapshot().app.secret.as_bytes(),
            crate::wiring::sales::sales_services(state.db(), state.rbac()),
        )))
        .register(DeliveryCoverageRefreshAdapter::new(DeliveryCoverageRefreshProcess::new(state.db())))
        .register(SellableProjectionRefreshAdapter::new(SellableProjectionRefresher::new(state.db()))))
}

/// 检测后台句柄退出，包括 panic；业务轮次返回错误不会结束其循环。
async fn wait_for_failure(tasks: &[JoinHandle<()>], other_finished: impl Fn() -> bool) {
    loop {
        if tasks.iter().any(JoinHandle::is_finished) || other_finished() {
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn monitor_detects_panics_and_other_worker_exit_but_not_running_work() {
        let tasks = vec![tokio::spawn(std::future::pending())];
        assert!(
            tokio::time::timeout(Duration::from_millis(10), wait_for_failure(&tasks, || false))
                .await
                .is_err()
        );
        tokio::time::timeout(Duration::from_secs(1), wait_for_failure(&tasks, || true)).await.unwrap();
        tasks[0].abort();
        let crashed = vec![tokio::spawn(async { panic!("worker failure fixture") })];
        tokio::time::timeout(Duration::from_secs(2), wait_for_failure(&crashed, || false)).await.unwrap();
        assert!(crashed.into_iter().next().unwrap().await.unwrap_err().is_panic());
    }
}
