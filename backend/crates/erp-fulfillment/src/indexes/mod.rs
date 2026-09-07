//! 履约九类集合的命名索引。

mod fulfillment;

/// 按既有集合顺序安装履约索引。
///
/// # Errors
/// 索引冲突或 MongoDB 操作失败时返回错误。
pub async fn ensure(db: &mongodb::Database) -> persistence_core::Result<()> {
    fulfillment::ensure(db).await
}
