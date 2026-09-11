//! 商品导入任务适配器：把商品导入流程接入统一后台执行器。

use super::adapter::BackgroundTaskAdapter;
use crate::product_import::ProductImportProcess;
use crate::Result;

/// 商品导入任务适配器。
pub struct ProductImportTaskAdapter {
    /// 商品导入流程。
    process: ProductImportProcess,
}

impl ProductImportTaskAdapter {
    /// 创建商品导入任务适配器。
    ///
    /// # 参数
    /// * `process` - 已装配数据库、对象存储与密钥的商品导入流程
    ///
    /// # 返回
    /// 返回可注册到统一执行器的适配器。
    pub fn new(process: ProductImportProcess) -> Self {
        Self { process }
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for ProductImportTaskAdapter {
    /// 返回商品导入任务名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回稳定的任务名 `product_import`。
    fn name(&self) -> &'static str {
        "product_import"
    }

    /// 领取并执行一轮到期商品导入任务。
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
