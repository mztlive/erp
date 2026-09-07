//! 采购集合索引注册。

mod procurement_responsibility;
mod purchase_order;

/// 按原相对顺序注册采购责任规则与采购单索引。
///
/// # Errors
/// 数据违反唯一约束或 MongoDB 索引操作失败时返回错误。
pub async fn ensure(db: &mongodb::Database) -> persistence_core::Result<()> {
    procurement_responsibility::ensure(db).await?;
    purchase_order::ensure(db).await
}
