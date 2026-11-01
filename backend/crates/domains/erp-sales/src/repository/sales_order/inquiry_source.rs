//! 询价来源独立集合；销售主表无来源的历史数据保持原行为。
use mongodb::Database;
use persistence_core::Repository;

use crate::entity::sales_order::inquiry_source::SalesOrderInquirySource;

/// 销售询价来源集合名称。
pub const SALES_INQUIRY_SOURCES: &str = "sales_order_inquiry_sources";

/// 读取本域询价来源仓储，ID 为销售单稳定身份。
/// # 参数
/// `db` 为本域数据库。
/// # 返回
/// 返回沿用调用方执行器的仓储。
/// # 错误
/// 无。
pub fn inquiry_sources(db: &Database) -> Repository<'_, SalesOrderInquirySource> {
    Repository::new(db, SALES_INQUIRY_SOURCES)
}
