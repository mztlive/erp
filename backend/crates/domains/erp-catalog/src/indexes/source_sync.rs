//! 商品资料来源唯一性和确认回执索引。
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::source_sync::repository::ProductSourceExt;

pub(super) async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(<Database as ProductSourceExt>::PRODUCT_SOURCES)
        .create_indexes([
            unique_index("uk_product_sources_id", doc! {"id":1}),
            unique_index("uk_product_sources_product", doc! {"product_id":1}),
            named_index("idx_product_sources_pending", doc! {"pending_count":1,"product_id":1}),
            named_index("idx_product_sources_catalog", doc! {"connection_id":1,"source_product_id":1}),
        ])
        .await?;
    db.collection::<Document>(<Database as ProductSourceExt>::SOURCE_RESOLUTION_RECEIPTS)
        .create_indexes([unique_index("uk_product_source_resolution_id", doc! {"id":1})])
        .await?;
    Ok(())
}
