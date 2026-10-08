//! 合同集合索引。

mod contract;
mod recognition;
mod templates;

use mongodb::Database;
use persistence_core::Result;

/// 登记合同、模板与识别集合的索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 无返回值。各集合的命名索引已登记。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    contract::ensure(db).await?;
    templates::ensure(db).await?;
    recognition::ensure(db).await
}

/// 只登记合同模板新增集合，供上线迁移命令复用领域索引定义。
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 索引登记成功返回空值。
/// # 错误
/// 已有数据违反唯一约束或数据库操作失败。
pub async fn ensure_templates(db: &Database) -> Result<()> {
    templates::ensure(db).await
}
