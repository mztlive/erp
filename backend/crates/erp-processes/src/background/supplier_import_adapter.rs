//! 供应商导入任务适配器：把供应商导入流程接入统一后台执行器。

use super::adapter::BackgroundTaskAdapter;
use crate::Result;
use crate::supplier_import::SupplierImportProcess;

/// 供应商导入任务适配器。
pub struct SupplierImportTaskAdapter {
    /// 供应商导入流程。
    process: SupplierImportProcess,
}

impl SupplierImportTaskAdapter {
    /// 创建供应商导入任务适配器。
    ///
    /// # 参数
    /// * `process` - 已装配数据库、对象存储与密钥的供应商导入流程
    ///
    /// # 返回
    /// 返回可注册到统一执行器的适配器。
    pub fn new(process: SupplierImportProcess) -> Self {
        Self { process }
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for SupplierImportTaskAdapter {
    /// 返回供应商导入任务名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回稳定的任务名 `supplier_import`。
    fn name(&self) -> &'static str {
        "supplier_import"
    }

    /// 领取并执行一轮到期供应商导入任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回本轮认领的任务数。
    ///
    /// # 错误
    /// 查询失败等轮询级错误；单个任务失败由流程内部记日志后继续，不中断整轮。
    async fn run_due(&self) -> Result<usize> {
        Ok(self.process.run_due_jobs().await?)
    }
}
