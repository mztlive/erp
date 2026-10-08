//! 仓库集合索引。

mod warehouse;

use mongodb::Database;
use persistence_core::Result;

/// 创建本域集合的幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 无返回值。索引已创建或已存在。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    warehouse::ensure(db).await
}
