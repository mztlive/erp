//! 财务集合索引注册。

mod cost;
mod payable;
mod receivable;

/// 按原相对注册顺序创建成本、应付与应收集合索引。
///
/// # Errors
/// 已有数据违反唯一索引或 MongoDB 创建索引失败时返回存储错误。
pub async fn ensure(db: &mongodb::Database) -> persistence_core::Result<()> {
    cost::ensure(db).await?;
    payable::ensure(db).await?;
    receivable::ensure(db).await
}
