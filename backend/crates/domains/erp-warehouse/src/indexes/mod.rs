//! 仓库集合索引。

mod warehouse;

use mongodb::Database;
use persistence_core::Result;

/// 启动时幂等注册本域命名索引；已有数据违反唯一约束或 MongoDB 创建失败时直接返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    warehouse::ensure(db).await
}
