//! 主体集合索引。

mod party;

use mongodb::Database;
use persistence_core::Result;

/// 创建主体集合索引。
///
/// 本域命名索引全部创建成功时返回 `Ok(())`。
///
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时，返回 `party::ensure` 的错误。
pub async fn ensure(db: &Database) -> Result<()> {
    party::ensure(db).await
}
