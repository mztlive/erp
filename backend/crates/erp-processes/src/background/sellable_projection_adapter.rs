//! 公司商品池预算投影刷新适配器：把投影刷新接入统一后台执行器。

use super::adapter::BackgroundTaskAdapter;
use crate::Result;
use crate::sellable_projection_refresh::SellableProjectionRefresher;

/// 公司商品池预算投影刷新适配器。
pub struct SellableProjectionRefreshAdapter {
    refresher: SellableProjectionRefresher,
}

impl SellableProjectionRefreshAdapter {
    /// 包装投影刷新。
    ///
    /// # 参数
    /// * `refresher` - 投影刷新
    ///
    /// # 返回
    /// 返回适配器。
    ///
    /// # 错误
    /// 无。
    pub fn new(refresher: SellableProjectionRefresher) -> Self {
        Self { refresher }
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for SellableProjectionRefreshAdapter {
    fn name(&self) -> &'static str {
        "sellable_sku_projection_refresh"
    }

    async fn run_due(&self) -> Result<usize> {
        Ok(self.refresher.run_due().await?)
    }
}
