//! 消费绑定、来源去重及恢复队列索引；证据不设置 TTL。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::repository::SupplierMessageExt;

pub(super) async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(Database::SUPPLIER_MESSAGE_CONSUMERS)
        .create_indexes([
            unique_index("uk_supplier_message_consumers_id", doc! {"id":1}),
            unique_index("uk_supplier_message_consumers_account", doc! {"account_scope":1}),
        ])
        .await?;

    db.collection::<Document>(Database::SUPPLIER_PULLED_MESSAGES)
        .create_indexes([
            unique_index("uk_supplier_pulled_messages_id", doc! {"id":1}),
            unique_index(
                "uk_supplier_pulled_messages_source",
                doc! {"account_scope":1,"source_message_id":1},
            ),
            named_index(
                "ix_supplier_pulled_messages_pending",
                doc! {
                    "account_scope":1,"connection_id":1,"state":1,"next_attempt_at":1,"received_at":1,"id":1,
                },
            ),
        ])
        .await?;
    Ok(())
}
