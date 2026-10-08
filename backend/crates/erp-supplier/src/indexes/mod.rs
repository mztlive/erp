//! 供应商集合索引。

mod supplier;

mod handover_receipt;

use mongodb::Database;
use persistence_core::Result;

use crate::portal::ensure_indexes as ensure_portal_indexes;

/// 创建供应商、交接回执与门户集合索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 返回
/// 三组索引都创建成功时返回 `Ok(())`。
///
/// # 错误
/// 唯一约束冲突或 MongoDB 创建索引失败时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    supplier::ensure(db).await?;
    handover_receipt::ensure(db).await?;
    ensure_portal_indexes(db).await
}
