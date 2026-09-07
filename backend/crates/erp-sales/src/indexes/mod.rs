//! 销售集合索引注册。

mod sales_order;
mod sales_review;

/// 按原相对注册顺序创建销售单与销售变更集合索引。
///
/// # Errors
/// 已有数据违反唯一索引或 MongoDB 创建索引失败时返回存储错误。
pub async fn ensure(db: &mongodb::Database) -> persistence_core::Result<()> {
    sales_order::ensure(db).await?;
    sales_review::ensure(db).await
}
