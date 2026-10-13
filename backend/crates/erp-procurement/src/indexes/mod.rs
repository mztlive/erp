//! 采购集合索引注册。

pub mod command_receipt;
mod procurement_responsibility;
mod purchase_email;
mod purchase_order;
mod purchase_source_update;

/// 按责任规则、采购单、命令回执、发件记录、依据更新记录的顺序注册索引。
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
    command_receipt::ensure(db).await?;
    purchase_email::ensure(db).await?;
    purchase_source_update::ensure(db).await
}
