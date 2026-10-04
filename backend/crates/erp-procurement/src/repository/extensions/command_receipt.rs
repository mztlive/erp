//! 采购独立命令回执集合访问器。

use mongodb::Database;

use crate::entity::purchase_order::PurchaseReceiptResult;
use crate::repository::command_receipt::PurchaseCommandReceiptRepository;

/// 采购领域独立命令回执集合访问器。
pub trait PurchaseCommandExt {
    /// 全部采购命令共用的不可变回执集合。
    const PURCHASE_COMMAND_RECEIPTS: &'static str = "purchase_command_receipts";
    /// 获取与结果类型绑定的拥有仓储。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回采购回执仓储，所有操作接收原 Executor。
    /// # 错误
    /// 无。
    fn purchase_command_receipts<T: PurchaseReceiptResult>(&self) -> PurchaseCommandReceiptRepository<'_, T>;
}
impl PurchaseCommandExt for Database {
    fn purchase_command_receipts<T: PurchaseReceiptResult>(&self) -> PurchaseCommandReceiptRepository<'_, T> {
        PurchaseCommandReceiptRepository::new(self, Self::PURCHASE_COMMAND_RECEIPTS)
    }
}
