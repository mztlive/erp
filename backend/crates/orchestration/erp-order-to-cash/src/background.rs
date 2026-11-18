//! 选品准备任务适配器：把选品准备流程接入统一后台执行器。

use background_runtime::{BackgroundTaskAdapter, TaskResult};

use crate::sales_selection::SalesSelectionProcess;

/// 选品准备任务适配器。
pub struct SalesSelectionTaskAdapter {
    /// 选品流程。
    process: SalesSelectionProcess,
}

impl SalesSelectionTaskAdapter {
    /// 应用注册选品准备任务时使用，包装已装配数据库、对象存储和密钥的选品流程，由统一执行器调度。
    pub fn new(process: SalesSelectionProcess) -> Self {
        Self { process }
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for SalesSelectionTaskAdapter {
    // 以稳定标识 `sales_selection_prepare` 注册和识别选品准备轮询任务。
    fn name(&self) -> &'static str {
        "sales_selection_prepare"
    }

    // 执行器触发轮询时领取并处理到期的选品准备任务，返回本轮处理数；查询等轮询级失败向执行器返回错误。
    async fn run_due(&self) -> TaskResult<usize> {
        Ok(self.process.run_due_prepare_tasks().await? as usize)
    }
}
