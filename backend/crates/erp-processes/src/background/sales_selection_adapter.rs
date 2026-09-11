//! 选品准备任务适配器：把选品准备流程接入统一后台执行器。

use super::adapter::BackgroundTaskAdapter;
use crate::sales_selection::SalesSelectionProcess;
use crate::Result;

/// 选品准备任务适配器。
pub struct SalesSelectionTaskAdapter {
    /// 选品流程。
    process: SalesSelectionProcess,
}

impl SalesSelectionTaskAdapter {
    /// 创建选品准备任务适配器。
    ///
    /// # 参数
    /// * `process` - 已装配数据库、对象存储与密钥的选品流程
    ///
    /// # 返回
    /// 返回可注册到统一执行器的适配器。
    pub fn new(process: SalesSelectionProcess) -> Self {
        Self { process }
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for SalesSelectionTaskAdapter {
    /// 返回选品准备任务名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回稳定的任务名 `sales_selection_prepare`。
    fn name(&self) -> &'static str {
        "sales_selection_prepare"
    }

    /// 领取并执行一轮到期选品准备任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回本轮处理的任务数。
    ///
    /// # 错误
    /// 查询失败等轮询级错误。
    async fn run_due(&self) -> Result<usize> {
        Ok(self.process.run_due_prepare_tasks().await? as usize)
    }
}
