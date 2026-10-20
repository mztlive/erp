//! 审批抄送集合索引。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::approval_copy::{CONFIGS, RECEIPTS};

/// 注册配置、幂等、收件排序与未读索引。
/// # 参数
/// 工作流数据库。
/// # 返回
/// 无。
/// # 错误
/// 索引创建失败。
pub async fn ensure_indexes(db: &Database) -> Result<()> {
    for (collection, indexes) in [
        (CONFIGS, vec![index("uk_copy_config_id", doc! { "id": 1 }, true)]),
        (
            RECEIPTS,
            vec![
                index("uk_copy_receipt_id", doc! { "id": 1 }, true),
                index("idx_copy_instance_history", doc! { "instance_id": 1, "id": 1 }, false),
                index(
                    "uk_copy_event_recipient",
                    doc! { "instance_id": 1, "event_key": 1, "recipient_user_id": 1 },
                    true,
                ),
                index(
                    "idx_copy_inbox",
                    doc! { "recipient_user_id": 1, "delivered": 1, "copied_at": -1, "id": -1 },
                    false,
                ),
                index(
                    "idx_copy_unread",
                    doc! { "recipient_user_id": 1, "delivered": 1, "read_at": 1 },
                    false,
                ),
            ],
        ),
    ] {
        db.collection::<Document>(collection).create_indexes(indexes).await?;
    }
    Ok(())
}
fn index(name: &str, keys: Document, unique: bool) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.to_owned()).unique(unique).build())
        .build()
}
