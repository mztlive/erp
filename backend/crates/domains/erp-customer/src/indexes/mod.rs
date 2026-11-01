//! 客户集合索引。

mod customer;

use mongodb::Database;
use persistence_core::Result;

/// 创建客户集合索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 本域命名索引创建完成。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    customer::ensure(db).await
}
