//! 导入领域拥有的独立命令回执集合。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::command_receipt::ImportCommandReceipt;

/// 命令回执访问器，不依赖展示审计集合。
pub trait ImportCommandReceiptExt {
    const IMPORT_COMMAND_RECEIPTS: &'static str = "import_command_receipts";
    /// 返回本域独立命令回执仓储。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回复用调用方 Executor 的仓储。
    ///
    /// # 错误
    /// 不返回错误。
    fn import_command_receipts(&self) -> Repository<'_, ImportCommandReceipt>;
}

impl ImportCommandReceiptExt for Database {
    fn import_command_receipts(&self) -> Repository<'_, ImportCommandReceipt> {
        Repository::new(self, Self::IMPORT_COMMAND_RECEIPTS)
    }
}
