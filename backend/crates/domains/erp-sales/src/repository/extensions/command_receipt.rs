//! 销售领域独立命令回执集合。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::command_receipt::SalesCommandReceipt;

/// 销售命令回执集合访问合同。
pub trait SalesCommandExt {
    const SALES_COMMAND_RECEIPTS: &'static str = "sales_command_receipts";
    /// 返回本域不可变回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回使用调用方 Executor 的仓储。
    /// # 错误
    /// 无。
    fn sales_command_receipts(&self) -> Repository<'_, SalesCommandReceipt>;
}
impl SalesCommandExt for Database {
    fn sales_command_receipts(&self) -> Repository<'_, SalesCommandReceipt> {
        Repository::new(self, Self::SALES_COMMAND_RECEIPTS)
    }
}
