//! 询价及独立命令回执集合访问器。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::sales_inquiry::{InquiryCommandReceipt, SalesInquiry};

pub trait SalesInquiryExt {
    const SALES_INQUIRIES: &'static str = "sales_inquiries";
    const SALES_INQUIRY_COMMAND_RECEIPTS: &'static str = "sales_inquiry_command_receipts";
    /// 取得询价聚合仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回执行器驱动的本域仓储。
    /// # 错误
    /// 无。
    fn sales_inquiries(&self) -> Repository<'_, SalesInquiry>;
    /// 取得只允许追加的命令回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回执行器驱动的本域仓储。
    /// # 错误
    /// 无。
    fn inquiry_command_receipts(&self) -> Repository<'_, InquiryCommandReceipt>;
}

impl SalesInquiryExt for Database {
    fn sales_inquiries(&self) -> Repository<'_, SalesInquiry> {
        Repository::new(self, Self::SALES_INQUIRIES)
    }
    fn inquiry_command_receipts(&self) -> Repository<'_, InquiryCommandReceipt> {
        Repository::new(self, Self::SALES_INQUIRY_COMMAND_RECEIPTS)
    }
}
