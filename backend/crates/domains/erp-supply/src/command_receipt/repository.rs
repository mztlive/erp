//! 独立供应链回执仓储，不以软删除隐藏仍承担去重的记录。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use super::SupplyCommandReceipt;

/// 供应链命令回执的唯一集合。
pub const SUPPLY_COMMAND_RECEIPTS: &str = "supply_command_receipts";

/// 本域回执的窄集合访问入口。
pub trait SupplyCommandReceiptExt {
    /// 获取本域独立回执仓储。
    /// 返回绑定当前数据库的仓储。
    fn supply_command_receipts(&self) -> Repository<'_, SupplyCommandReceipt>;
}

impl SupplyCommandReceiptExt for Database {
    fn supply_command_receipts(&self) -> Repository<'_, SupplyCommandReceipt> {
        Repository::new(self, SUPPLY_COMMAND_RECEIPTS)
    }
}

/// 不过滤软删除的命令定位读取。
#[allow(async_fn_in_trait)]
pub trait SupplyCommandReceiptReadExt {
    /// 用原事务或查证执行器按稳定命令 ID 查找回执，供提交结果未知时查证；查询不排除软删除命中，调用方验
    /// 证后才能恢复。读库或反序列化失败不当作未执行。
    async fn find_command(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplyCommandReceipt>>;
}

impl SupplyCommandReceiptReadExt for Repository<'_, SupplyCommandReceipt> {
    async fn find_command(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplyCommandReceipt>> {
        mongo_ops::find_one(&self.collection(), doc! { "id": id }, executor).await
    }
}
