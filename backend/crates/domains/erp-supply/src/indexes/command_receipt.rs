//! 成功供应链命令的稳定唯一防护。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::command_receipt::repository::SUPPLY_COMMAND_RECEIPTS;

/// 登记回执唯一索引，不沿审计归档删除。
///
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 索引登记成功返回空结果。
/// # 错误
/// 唯一冲突或数据库错误时失败。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(SUPPLY_COMMAND_RECEIPTS).create_indexes([unique_command_index()]).await?;
    Ok(())
}

/// 命令唯一保护长期保留，不随审计展示周期过期。
fn unique_command_index() -> IndexModel {
    IndexModel::builder()
        .keys(doc! { "id": 1 })
        .options(
            IndexOptions::builder().name("uk_supply_command_receipts_id".to_string()).unique(true).build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_identity_is_unique_and_has_no_expiration() {
        let index = unique_command_index();
        assert_eq!(index.keys, doc! { "id": 1 });
        let options = index.options.unwrap();
        assert_eq!(options.unique, Some(true));
        assert_eq!(options.name.as_deref(), Some("uk_supply_command_receipts_id"));
        assert!(options.expire_after.is_none());
    }
}
