//! 索引存在性断言辅助。

use std::collections::HashSet;

use mongodb::Database;
use mongodb::bson::Document;

use crate::{Error, Result};

/// 断言指定集合存在所有给定名称的命名索引。
///
/// 通过 `listIndexes` 获取集合全部索引名（含内建 `_id_`），校验其中是否
/// 包含全部期望名称。
///
/// # 参数
/// * `db` - 目标数据库
/// * `collection` - 集合名
/// * `names` - 期望存在的索引名列表
///
/// # 返回
/// 全部索引存在时返回 `Ok(())`。
///
/// # 错误
/// 任一索引缺失或 MongoDB 查询失败时返回错误。
pub async fn assert_indexes(db: &Database, collection: &str, names: &[&str]) -> Result<()> {
    let existing: HashSet<String> =
        db.collection::<Document>(collection).list_index_names().await?.into_iter().collect();
    let missing: Vec<_> =
        names.iter().copied().filter(|name| !existing.contains(*name)).map(str::to_owned).collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Error::IndexMissing { collection: collection.to_string(), missing })
    }
}
