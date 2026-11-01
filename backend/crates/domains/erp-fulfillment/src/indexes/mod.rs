//! 履约九类集合的命名索引。

mod command_receipt;
mod fulfillment;

/// 按既有集合顺序安装履约索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 索引冲突或 MongoDB 操作失败时返回错误。
pub async fn ensure(db: &mongodb::Database) -> persistence_core::Result<()> {
    fulfillment::ensure(db).await?;
    command_receipt::ensure(db).await
}
