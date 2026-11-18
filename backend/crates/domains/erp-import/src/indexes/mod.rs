//! 导入集合索引。

mod command_receipt;
mod legacy_import;

use mongodb::Database;
use persistence_core::Result;

/// 在启动时注册历史导入和独立命令回执的索引；已有数据违反唯一约束或 MongoDB 创建失败时返回错误，不能继续假定约束已生效。
pub async fn ensure(db: &Database) -> Result<()> {
    legacy_import::ensure(db).await?;
    command_receipt::ensure(db).await
}
