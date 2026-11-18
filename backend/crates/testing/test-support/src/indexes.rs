//! 索引存在性断言辅助。

use std::collections::HashSet;

use mongodb::Database;
use mongodb::bson::Document;

use crate::{Error, Result};

/// 在已有数据库的索引断言中，通过 `listIndexes` 检查指定集合是否包含全部期望名称（含内建 `_id_`）。
/// 仅比较名称，不验证索引键、唯一性或查询是否使用索引。查询失败或有缺项时返回错误；缺项错误列出具体名称。
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
