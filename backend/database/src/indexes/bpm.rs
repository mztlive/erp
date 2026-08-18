//! BPM 目标集合索引。P2 填充真实索引。

use mongodb::Database;

use crate::Result;

/// 目标 BPM 集合尚未落地，本占位不创建索引。
///
/// # 参数
/// * `_db` - 目标 MongoDB 数据库
///
/// # 错误
/// 本占位不返回错误；调用成功也不表示集合或索引已就绪。
pub(crate) async fn ensure(_db: &Database) -> Result<()> {
    Ok(())
}
