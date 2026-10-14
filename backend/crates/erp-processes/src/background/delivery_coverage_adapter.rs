//! SKU 配送覆盖刷新适配器：把覆盖刷新流程接入统一后台执行器。

use super::adapter::BackgroundTaskAdapter;
use crate::Result;
use crate::delivery_coverage_refresh::DeliveryCoverageRefreshProcess;

/// SKU 配送覆盖刷新适配器。
pub struct DeliveryCoverageRefreshAdapter {
    process: DeliveryCoverageRefreshProcess,
}

impl DeliveryCoverageRefreshAdapter {
    /// 包装刷新流程。
    ///
    /// # 参数
    /// * `process` - 覆盖刷新流程
    ///
    /// # 返回
    /// 返回适配器。
    ///
    /// # 错误
    /// 无。
    pub fn new(process: DeliveryCoverageRefreshProcess) -> Self {
        Self { process }
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for DeliveryCoverageRefreshAdapter {
    fn name(&self) -> &'static str {
        "sku_delivery_coverage_refresh"
    }

    async fn run_due(&self) -> Result<usize> {
        self.process.run_due().await
    }
}
