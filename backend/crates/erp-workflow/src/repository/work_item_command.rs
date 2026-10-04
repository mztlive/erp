//! 工作项命令回执集合的窄访问器，读取不隐藏损坏或已删除身份。
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::work_item_command::WorkItemCommandReceipt;

pub trait WorkItemCommandExt {
    const WORK_ITEM_COMMAND_RECEIPTS: &'static str = "work_item_command_receipts";
    /// 取得工作项领域独立回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回本域不可变回执仓储。
    /// # 错误
    /// 无。
    fn work_item_command_receipts(&self) -> Repository<'_, WorkItemCommandReceipt>;
}
impl WorkItemCommandExt for Database {
    fn work_item_command_receipts(&self) -> Repository<'_, WorkItemCommandReceipt> {
        Repository::new(self, Self::WORK_ITEM_COMMAND_RECEIPTS)
    }
}
#[allow(async_fn_in_trait)]
pub trait WorkItemCommandRepositoryExt {
    /// 按稳定命令身份查证原结果。
    /// # 参数
    /// * `id` - 当前命令身份。
    /// * `executor` - 与调用方相同的执行器。
    /// # 返回
    /// 返回原回执或未命中。
    /// # 错误
    /// 数据库或结果反序列化失败时返回错误。
    async fn find_command(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItemCommandReceipt>>;
}
impl WorkItemCommandRepositoryExt for Repository<'_, WorkItemCommandReceipt> {
    async fn find_command(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItemCommandReceipt>> {
        mongo_ops::find_one(&self.collection(), doc! { "id": id }, executor).await
    }
}
