use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::SupplierCallbackExt;

pub(super) async fn ensure(db: &Database) -> Result<()> {
    let unique = IndexModel::builder()
        .keys(doc! {"id":1})
        .options(
            IndexOptions::builder().name("uk_supplier_callback_receipts_id".to_string()).unique(true).build(),
        )
        .build();
    let pending = IndexModel::builder()
        .keys(doc! {"connection_id":1,"status":1,"received_at":1,"id":1})
        .options(IndexOptions::builder().name("ix_supplier_callback_receipts_pending".to_string()).build())
        .build();
    db.collection::<Document>(Database::SUPPLIER_CALLBACK_RECEIPTS).create_indexes([unique, pending]).await?;
    Ok(())
}
