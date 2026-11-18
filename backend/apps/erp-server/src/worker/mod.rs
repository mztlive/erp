//! 常驻任务装配与监督；所有循环共享进程退出边界。
mod approval_outbox;
mod data_transfer;
mod resource_grants;
mod supplier_orders;
mod xxl;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use background_runtime::BackgroundRunner;
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

use self::approval_outbox::ApprovalOutboxWorker;
use self::supplier_orders::SupplierOrderWorker;
use self::xxl::XxlExecutor;
use crate::runtime::Result;
use crate::wiring::sales::sales_services;
use crate::wiring::{Application, scoped_catalog_service};

/// 进程唯一的常驻执行器集合。
pub(crate) struct Workers {
    stop: watch::Sender<bool>,
    xxl: Arc<XxlExecutor>,
    tasks: Vec<JoinHandle<()>>,
    catalog: CatalogTaskControl,
    orders: SupplierOrderWorker,
    outbox: ApprovalOutboxWorker,
}

impl Workers {
    // 先完成所有可失败装配，再启动任何循环。
    // 进程唯一的常驻执行器集合。
    // 依赖装配或旧调度回执恢复失败时拒绝启动。
    pub(crate) async fn start(state: Application) -> Result<Self> {
        let config = state.config_snapshot().xxl_job.ok_or("Worker 必须配置 [xxl_job]")?;
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
                scoped_catalog_service(state.db(), state.authorization()),
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
        tasks.push(tokio::spawn(resource_grants::run(state.clone(), receive.clone())));
        tasks.push(tokio::spawn(data_transfer::run(state.clone(), receive)));
        let orders = supplier_orders::spawn(state.clone());
        let outbox = ApprovalOutboxWorker::start(state.approval_outbox_port());
        Ok(Self { stop, tasks, catalog, orders, outbox, xxl })
    }

    // 独立 Worker 必备的 XXL 协议路由。
    // 执行器协议路由。
    pub(crate) fn scheduler_routes(&self) -> Router {
        self.xxl.router()
    }

    // 把意外结束的常驻任务作为进程退出信号；轮次业务错误留给各循环处理，不触发此分支。
    pub(crate) async fn failed(&self) {
        wait_for_failure(&self.tasks, || self.orders.is_finished() || self.outbox.is_finished()).await;
    }

    // 进入停机时同时关闭所有认领或受理入口，以免等待第一项任务时其他循环继续领新活。
    pub(crate) fn request_stop(&self) {
        self.xxl.request_stop();
        self.stop.send_replace(true);
        self.catalog.request_stop();
        self.orders.request_stop();
        self.outbox.request_stop();
    }

    // 关停入口后等待各 Worker 完成已开始的轮次；时间上限由进程统一排空预算控制。
    pub(crate) async fn drain(self) {
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

// Worker 启动时按既有认领顺序注册导入、供应商目录采纳、选品及投影刷新适配器；目录流程装配失败时不启动消费循环。
fn build_runner(state: &Application) -> Result<BackgroundRunner> {
    Ok(BackgroundRunner::new(Duration::from_secs(2))
        .register(state.product_import_process())
        .register(state.supplier_import_process())
        .register(state.brand_import_process())
        .register(SupplierCatalogAdoptionTaskAdapter::new(
            state.db(),
            state.supplier_catalog_adoption_process()?,
        ))
        .register(SalesSelectionTaskAdapter::new(SalesSelectionProcess::new(
            state.db(),
            state.storage().clone(),
            state.config_snapshot().app.secret.as_bytes(),
            sales_services(state.db(), state.rbac(), state.authorization()),
        )))
        .register(DeliveryCoverageRefreshAdapter::new(DeliveryCoverageRefreshProcess::new(state.db())))
        .register(SellableProjectionRefreshAdapter::new(SellableProjectionRefresher::new(state.db()))))
}

// 监督长驻句柄何时结束，包括 panic；正常轮次错误由循环记录，不作为退出信号。
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
