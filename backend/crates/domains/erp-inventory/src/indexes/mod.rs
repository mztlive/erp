//! 库存集合索引。

mod inventory;

use mongodb::Database;
use persistence_core::Result;

/// 创建库存集合索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 索引创建完成。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 创建索引失败时，返回下层错误。
pub async fn ensure(db: &Database) -> Result<()> {
    inventory::ensure(db).await
}
