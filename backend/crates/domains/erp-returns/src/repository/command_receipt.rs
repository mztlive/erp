//! 退货逆向命令回执集合访问器及拥有仓储。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::command_receipt::ReturnsCommandReceipt;

/// 本领域命令回执仓储，所有操作复用调用方 Executor。
pub type ReturnsCommandReceiptRepository<'a> = Repository<'a, ReturnsCommandReceipt>;
/// 退货逆向独立命令回执集合合同。
pub trait ReturnsCommandExt {
    /// 本领域独立回执集合。
    const COMMAND_RECEIPTS: &'static str = "returns_command_receipts";
    /// 返回绑定拥有集合的仓储。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回独立回执仓储。
    /// # 错误
    /// 无。
    fn command_receipts(&self) -> ReturnsCommandReceiptRepository<'_>;
}
impl ReturnsCommandExt for Database {
    fn command_receipts(&self) -> ReturnsCommandReceiptRepository<'_> {
        ReturnsCommandReceiptRepository::new(self, Self::COMMAND_RECEIPTS)
    }
}
