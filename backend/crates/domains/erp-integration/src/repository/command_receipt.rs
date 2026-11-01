//! W29 领域独立命令回执仓储，全部操作复用调用方执行器。
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::integration_ops::IntegrationCommandReceipt;
pub trait IntegrationCommandExt {
    const INTEGRATION_COMMAND_RECEIPTS: &'static str = "integration_command_receipts";
    /// 取得独立集成命令回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回本域回执仓储。
    /// # 错误
    /// 无。
    fn integration_command_receipts(&self) -> Repository<'_, IntegrationCommandReceipt>;
}
impl IntegrationCommandExt for Database {
    fn integration_command_receipts(&self) -> Repository<'_, IntegrationCommandReceipt> {
        Repository::new(self, Self::INTEGRATION_COMMAND_RECEIPTS)
    }
}
#[allow(async_fn_in_trait)]
pub trait IntegrationCommandRepositoryExt {
    /// 按唯一命令身份查证原结果，不以软删除隐藏占用。
    /// # 参数
    /// * `id` - 当前命令身份。
    /// * `executor` - 当前事务或查证执行器。
    /// # 返回
    /// 返回原回执或未命中。
    /// # 错误
    /// 数据库与反序列化错误保持原分类。
    async fn find_command(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<IntegrationCommandReceipt>>;
}
impl IntegrationCommandRepositoryExt for Repository<'_, IntegrationCommandReceipt> {
    async fn find_command(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<IntegrationCommandReceipt>> {
        mongo_ops::find_one(&self.collection(), doc! { "id": id }, executor).await
    }
}
