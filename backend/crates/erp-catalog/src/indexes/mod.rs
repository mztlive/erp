//! 商品域集合索引。

mod barcode_claim;
mod catalog;

mod handover_receipt;
mod portal;

use mongodb::Database;
use persistence_core::Result;

/// 创建商品域集合索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 返回
/// 商品、条码占用、交接回执和门户索引都登记成功时无返回值。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    catalog::ensure(db).await?;
    barcode_claim::ensure(db).await?;
    handover_receipt::ensure(db).await?;
    portal::ensure(db).await
}
