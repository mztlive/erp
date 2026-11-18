//! SKU 配送覆盖刷新适配器：把覆盖刷新流程接入统一后台执行器。

use background_runtime::{BackgroundTaskAdapter, TaskResult};

use crate::delivery_coverage_refresh::DeliveryCoverageRefreshProcess;

/// SKU 配送覆盖刷新适配器。
pub struct DeliveryCoverageRefreshAdapter {
    process: DeliveryCoverageRefreshProcess,
}

impl DeliveryCoverageRefreshAdapter {
    /// 在后台运行时登记 SKU 配送覆盖刷新时，绑定已装配的刷新流程；构造适配器本身不刷新。
    pub fn new(process: DeliveryCoverageRefreshProcess) -> Self {
        Self { process }
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for DeliveryCoverageRefreshAdapter {
    fn name(&self) -> &'static str {
        "sku_delivery_coverage_refresh"
    }

    async fn run_due(&self) -> TaskResult<usize> {
        Ok(self.process.run_due().await?)
    }
}
