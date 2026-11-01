//! 发件身份永久唯一，不使用 TTL 或软删除条件。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::purchase_email::PURCHASE_EMAILS;
pub(super) async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(PURCHASE_EMAILS)
        .create_index(
            IndexModel::builder()
                .keys(doc! { "id": 1 })
                .options(
                    IndexOptions::builder().name("uk_purchase_email_id".to_string()).unique(true).build(),
                )
                .build(),
        )
        .await?;
    db.collection::<Document>(PURCHASE_EMAILS)
        .create_index(
            IndexModel::builder()
                .keys(doc! { "purchase_order_id": 1, "created_at": -1, "id": -1 })
                .options(IndexOptions::builder().name("ix_purchase_email_order".to_string()).build())
                .build(),
        )
        .await?;
    Ok(())
}
