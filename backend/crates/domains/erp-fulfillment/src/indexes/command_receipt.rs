//! 履约独立命令回执唯一约束，不随展示日志过期。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::FulfillmentCommandExt;
/// 原命令身份唯一索引。
pub const COMMAND_RECEIPT_ID_INDEX: &str = "uk_fulfillment_command_receipts_id";
/// 安装独立命令身份唯一索引。
///
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 成功时返回空结果。
/// # 错误
/// 唯一约束不满足或Mongo创建失败时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    let index = IndexModel::builder()
        .keys(doc! { "id": 1 })
        .options(IndexOptions::builder().name(COMMAND_RECEIPT_ID_INDEX.to_string()).unique(true).build())
        .build();
    db.collection::<Document>(Database::COMMAND_RECEIPTS).create_index(index).await?;
    Ok(())
}
