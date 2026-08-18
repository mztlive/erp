//! ERP 审批集成集合索引。P2 填充快照与 outbox 索引。

use mongodb::Database;

use crate::Result;

/// 目标集成集合尚未落地，本占位不创建索引。
///
/// # 参数
/// * `_db` - 目标 MongoDB 数据库
///
/// # 错误
/// 本占位不返回错误；调用成功也不表示集合或索引已就绪。
pub(crate) async fn ensure(_db: &Database) -> Result<()> {
    Ok(())
}
