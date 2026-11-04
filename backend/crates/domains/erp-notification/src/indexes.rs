use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::repository::COLLECTION;

/// 创建通知身份、去重、个人时间线及未读计数索引。
/// # 参数
/// db 为目标数据库。
/// # 返回
/// 全部索引已创建。
/// # 错误
/// 数据冲突或索引创建失败时返回仓储错误。
pub async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(COLLECTION)
        .create_indexes([
            unique_index("uk_notifications_id", doc! { "id": 1 }),
            unique_index("uk_notifications_recipient_event", doc! { "recipient_id": 1, "dedup_key": 1 }),
            named_index("idx_notifications_inbox", doc! { "recipient_id": 1, "created_at": -1, "id": -1 }),
            named_index("idx_notifications_unread", doc! { "recipient_id": 1, "read_at": 1 }),
        ])
        .await?;
    Ok(())
}
