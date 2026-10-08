//! 销售集合索引注册。

mod command_receipt;
mod sales_order;
mod sales_review;
mod sales_selection;

/// 按原相对注册顺序创建销售单、销售变更、选品与命令回执集合索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 全部索引创建或已存在时返回空结果。
///
/// # 错误
/// 已有数据违反唯一索引或 MongoDB 创建索引失败时返回存储错误。
pub async fn ensure(db: &mongodb::Database) -> persistence_core::Result<()> {
    sales_order::ensure(db).await?;
    sales_review::ensure(db).await?;
    sales_selection::ensure(db).await?;
    command_receipt::ensure(db).await
}
