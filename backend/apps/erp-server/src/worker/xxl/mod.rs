//! 独立 Worker 的 XXL 协议和运行时装配。
mod admin;
mod protocol;
mod routes;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use background_runtime::periodic;
use chrono::Utc;
use config::XxlJobConfig;
use erp_supplier_catalog::background::SupplierCatalogTaskAdapter;
use erp_supplier_orders::supplier_order_tracking::SupplierOrderTrackingProcess;
use erp_support::entity::scheduled_run::ScheduledHandler;
use erp_support::repository::scheduled_runs::ScheduledRunExt;
use erp_support::service::scheduled_execution::{ScheduledExecutionProcess, ScheduledWork};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use self::admin::XxlAdmin;
use crate::runtime::Result;
use crate::wiring::Application;

/// 固定启动配置与持久化执行入口。
pub(crate) struct XxlExecutor {
    config: XxlJobConfig,
    process: Arc<ScheduledExecutionProcess>,
    admin: Arc<XxlAdmin>,
    stopping: AtomicBool,
}
impl XxlExecutor {
    // 完成恢复后才允许注册、监听和认领。
    // 已恢复旧回执的执行器。
    // 能力未装配、HTTP 客户端或持久化恢复失败时拒绝启动。
    pub(crate) async fn new(
        state: &Application,
        config: XxlJobConfig,
        catalog: Arc<SupplierCatalogTaskAdapter>,
    ) -> Result<Arc<Self>> {
        let tracking =
            state.supplier_order_tracking_process().ok_or("Worker 未装配供应商跟踪能力，不能启动履约任务")?;
        let admin = Arc::new(XxlAdmin::new(config.clone())?);
        let work = Arc::new(Jobs { catalog, tracking });
        let process = Arc::new(ScheduledExecutionProcess::new(
            config.app_name.clone(),
            id_generator::next_id(),
            Arc::new(state.db().scheduled_runs()),
            work,
            admin.clone(),
        ));
        process.recover().await?;
        Ok(Arc::new(Self { config, process, admin, stopping: AtomicBool::new(false) }))
    }

    // 返回仅用于内部 Service 的协议路由。
    // self 为共享执行器。
    // 带 token 校验的固定协议路由。
    pub(crate) fn router(self: &Arc<Self>) -> Router {
        routes::router(self.clone())
    }

    // 同时停止接收；轮次通过共享 stop 通知在边界排空。
    // 新协议请求开始被拒绝。
    pub(crate) fn request_stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    // 启动独立注册、执行和回报循环，避免长业务阻塞心跳。
    // 由 Worker 监督和排空的三个句柄。
    // 后台存储错误由监督器处理。
    pub(crate) fn spawn(self: &Arc<Self>, stop: watch::Receiver<bool>) -> Vec<JoinHandle<()>> {
        let registration = self.clone();
        let registry_stop = stop.clone();
        let execution = self.clone();
        let execute_stop = stop.clone();
        let callback = self.clone();
        vec![
            tokio::spawn(async move {
                periodic(registry_stop, Duration::from_secs(30), || async {
                    if !registration.admin.registry(false).await {
                        tracing::warn!("XXL 执行器注册失败，稍后重试；本地调度不会接管");
                    }
                })
                .await;
                if !registration.admin.registry(true).await {
                    tracing::warn!("XXL 执行器注销未确认，将等待注册超时失效");
                }
            }),
            tokio::spawn(async move {
                execution.execute(execute_stop).await;
            }),
            tokio::spawn(async move {
                periodic(stop, Duration::from_secs(5), || async {
                    if let Err(error) = callback.process.callbacks(Utc::now().timestamp()).await {
                        tracing::warn!(%error, "XXL 回报保存未确认，保留原记录重试");
                    }
                })
                .await;
            }),
        ]
    }

    // 领取并执行持久化调度回执；停止信号到来时不再领新工作。保存失败导致循环退出并触发进程监督，后续核对原运行回执而不是自动重放业务。
    async fn execute(&self, mut stop: watch::Receiver<bool>) {
        loop {
            if *stop.borrow() || stop.has_changed().is_err() {
                return;
            }
            if let Err(error) = self.process.run_next().await {
                tracing::error!(%error, "XXL 执行循环退出，原操作禁止自动重放");
                return;
            }
            tokio::select! { biased; _ = stop.changed() => {}, _ = tokio::time::sleep(Duration::from_secs(1)) => {} }
        }
    }
}

/// 固定 handler 到业务 Process 的映射。
struct Jobs {
    catalog: Arc<SupplierCatalogTaskAdapter>,
    tracking: SupplierOrderTrackingProcess,
}
#[async_trait]
impl ScheduledWork for Jobs {
    async fn run(&self, handler: ScheduledHandler) -> std::result::Result<usize, &'static str> {
        const UNCONFIRMED: &str = "本轮扫描未确认完成，请核对 ERP 原记录和执行日志";
        match handler {
            ScheduledHandler::Catalog => self.catalog.schedule_due().await.map_err(|_| UNCONFIRMED),
            ScheduledHandler::Tracking => self.tracking.poll_due().await.map_err(|_| UNCONFIRMED),
        }
    }
}
