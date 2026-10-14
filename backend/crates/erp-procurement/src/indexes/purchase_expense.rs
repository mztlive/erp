//! 费用行确认发生：同一采购单的同一费用行身份永久唯一，按销售单读取附加费用。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::purchase_expense::PURCHASE_EXPENSE_OCCURRENCES;

pub(super) async fn ensure(db: &Database) -> Result<()> {
    let collection = db.collection::<Document>(PURCHASE_EXPENSE_OCCURRENCES);
    collection
        .create_index(
            IndexModel::builder()
                .keys(doc! { "purchase_order_id": 1, "expense_line_id": 1 })
                .options(
                    IndexOptions::builder()
                        .name("uk_purchase_expense_occurrence_line".to_string())
                        .unique(true)
                        .build(),
                )
                .build(),
        )
        .await?;
    collection
        .create_index(
            IndexModel::builder()
                .keys(doc! { "sales_order_id": 1 })
                .options(
                    IndexOptions::builder()
                        .name("ix_purchase_expense_occurrence_sales_order".to_string())
                        .build(),
                )
                .build(),
        )
        .await?;
    Ok(())
}
