//! 财务命令回执集合名与拥有仓储访问器。

use mongodb::Database;

use crate::repository::command_receipt::FinanceCommandReceiptRepository;

/// 财务独立命令回执集合访问合同。
pub trait FinanceCommandExt {
    /// 财务命令回执集合名。
    const FINANCE_COMMAND_RECEIPTS: &'static str = "finance_command_receipts";

    /// 从当前数据库取得返回绑定本域集合的命令回执仓储，供相关用例复用调用方执行器。
    fn finance_command_receipts(&self) -> FinanceCommandReceiptRepository<'_>;
}

impl FinanceCommandExt for Database {
    fn finance_command_receipts(&self) -> FinanceCommandReceiptRepository<'_> {
        FinanceCommandReceiptRepository::new(self, Self::FINANCE_COMMAND_RECEIPTS)
    }
}
