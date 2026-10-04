//! 退货域集合索引，按原集合顺序创建。

mod command_receipt;
mod returns;

pub use command_receipt::COMMAND_RECEIPT_ID_INDEX;
use mongodb::Database;
use persistence_core::Result;

/// 按原集合顺序安装逆向事实及独立回执索引。
///
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 全部索引创建成功时返回空结果。
/// # 错误
/// 唯一约束不满足或 MongoDB 创建失败时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    returns::ensure(db).await?;
    command_receipt::ensure(db).await
}
