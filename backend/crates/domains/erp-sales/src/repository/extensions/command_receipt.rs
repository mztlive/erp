//! 销售领域独立命令回执集合。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::command_receipt::SalesCommandReceipt;
use crate::entity::sales_review::SalesChangeSubmissionReceipt;

/// 销售命令回执集合访问合同。
pub trait SalesCommandExt {
    const SALES_COMMAND_RECEIPTS: &'static str = "sales_command_receipts";
    /// 从当前数据库取得返回本域不可变回执仓储，供相关用例复用调用方执行器。
    fn sales_command_receipts(&self) -> Repository<'_, SalesCommandReceipt>;
    /// 按独立命令前缀读取销售变更提交回执，复用既有稳定ID唯一索引。
    /// 返回只按精确命令ID读写的强类型仓储。
    fn sales_change_submission_receipts(&self) -> Repository<'_, SalesChangeSubmissionReceipt>;
}
impl SalesCommandExt for Database {
    fn sales_change_submission_receipts(&self) -> Repository<'_, SalesChangeSubmissionReceipt> {
        Repository::new(self, Self::SALES_COMMAND_RECEIPTS)
    }

    fn sales_command_receipts(&self) -> Repository<'_, SalesCommandReceipt> {
        Repository::new(self, Self::SALES_COMMAND_RECEIPTS)
    }
}
