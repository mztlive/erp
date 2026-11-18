//! 询价及独立命令回执集合访问器。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::sales_inquiry::{InquiryCommandReceipt, SalesInquiry};

pub trait SalesInquiryExt {
    const SALES_INQUIRIES: &'static str = "sales_inquiries";
    const SALES_INQUIRY_COMMAND_RECEIPTS: &'static str = "sales_inquiry_command_receipts";
    /// 从当前数据库取得取得询价聚合仓储，供相关用例复用调用方执行器。
    fn sales_inquiries(&self) -> Repository<'_, SalesInquiry>;
    /// 从当前数据库取得取得只允许追加的命令回执仓储，供相关用例复用调用方执行器。
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
