//! 销售命令身份唯一性；回执不跟随审计归档删除。
use mongodb::bson::Document;
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::SalesCommandExt;

/// 登记销售命令 ID 唯一索引。
/// 结果：索引存在或创建成功时返回空结果。
/// 失败条件：身份重复或存储失败时拒绝启动。
///
/// 输入约定：`db` 为目标数据库。
pub async fn ensure(db: &Database) -> Result<()> {
    let keys = Document::from_iter([("id".to_string(), 1.into())]);
    let index = IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder().name("uk_sales_command_receipts_id".to_string()).unique(true).build(),
        )
        .build();
    db.collection::<Document>(Database::SALES_COMMAND_RECEIPTS).create_index(index).await?;
    Ok(())
}
