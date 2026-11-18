//! 公司商品池预算投影刷新适配器：把投影刷新接入统一后台执行器。

use background_runtime::{BackgroundTaskAdapter, TaskResult};

use crate::sellable_projection_refresh::SellableProjectionRefresher;

/// 公司商品池预算投影刷新适配器。
pub struct SellableProjectionRefreshAdapter {
    refresher: SellableProjectionRefresher,
}

impl SellableProjectionRefreshAdapter {
    /// 包装投影刷新。`refresher`：投影刷新。
    pub fn new(refresher: SellableProjectionRefresher) -> Self {
        Self { refresher }
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for SellableProjectionRefreshAdapter {
    fn name(&self) -> &'static str {
        "sellable_sku_projection_refresh"
    }

    async fn run_due(&self) -> TaskResult<usize> {
        Ok(self.refresher.run_due().await?)
    }
}
