//! 请求去重及本人历史分页索引。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::recognition::IMPORTS;

pub(super) async fn ensure(db: &Database) -> Result<()> {
    let indexes = [
        ("uk_contract_imports_id", doc! { "id": 1 }, true),
        ("uk_contract_imports_request", doc! { "owner_id": 1, "command.request_key": 1 }, true),
        ("idx_contract_imports_owner", doc! { "owner_id": 1, "created_at": -1, "id": -1 }, false),
    ];
    for (name, keys, unique) in indexes {
        db.collection::<Document>(IMPORTS)
            .create_index(
                IndexModel::builder()
                    .keys(keys)
                    .options(IndexOptions::builder().name(name.to_string()).unique(unique).build())
                    .build(),
            )
            .await?;
    }
    Ok(())
}
