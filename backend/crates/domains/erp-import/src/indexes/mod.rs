//! 导入集合索引。

mod command_receipt;
mod legacy_import;

use mongodb::Database;
use persistence_core::Result;

/// 创建导入集合索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 旧数据导入集合与命令回执集合的索引都建立完成后无返回值。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 创建索引失败时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    legacy_import::ensure(db).await?;
    command_receipt::ensure(db).await
}
