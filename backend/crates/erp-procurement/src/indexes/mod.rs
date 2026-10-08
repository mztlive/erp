//! 采购集合索引注册。

pub mod command_receipt;
mod procurement_responsibility;
mod purchase_order;

/// 按责任规则、采购单、命令回执的顺序注册索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 全部索引注册成功时无返回值。
///
/// # 错误
/// 数据违反唯一约束或 MongoDB 索引操作失败时返回错误。
pub async fn ensure(db: &mongodb::Database) -> persistence_core::Result<()> {
    procurement_responsibility::ensure(db).await?;
    purchase_order::ensure(db).await?;
    command_receipt::ensure(db).await
}
