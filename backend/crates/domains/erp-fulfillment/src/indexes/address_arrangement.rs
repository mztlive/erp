//! 履约独立收货安排唯一约束，不随展示日志过期。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::AddressArrangementExt;
/// 销售单身份唯一索引。
pub const ADDRESS_ARRANGEMENT_ID_INDEX: &str = "uk_fulfillment_address_arrangements_id";
/// 安装独立销售单身份唯一索引。
/// 成功时返回空结果。
/// 唯一约束不满足或Mongo创建失败时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    let index = IndexModel::builder()
        .keys(doc! { "id": 1 })
        .options(IndexOptions::builder().name(ADDRESS_ARRANGEMENT_ID_INDEX.to_string()).unique(true).build())
        .build();
    db.collection::<Document>(Database::ADDRESS_ARRANGEMENTS).create_index(index).await?;
    Ok(())
}
