//! ERP 审批集成集合索引。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::ApprovalIntegrationExt;
use crate::Result;

const SNAPSHOTS: &str = <mongodb::Database as ApprovalIntegrationExt>::APPROVAL_SUBJECT_SNAPSHOTS;
const OUTBOX: &str = <mongodb::Database as ApprovalIntegrationExt>::APPROVAL_NOTIFICATION_OUTBOX;

/// 为审批集成快照与通知 outbox 创建幂等命名索引。
///
/// 命令收据属于 BPM 持久化边界，其全部索引只允许由 `indexes::bpm` 声明。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 既有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SNAPSHOTS, snapshot_indexes()).await?;
    create_indexes(db, OUTBOX, outbox_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

fn snapshot_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_subject_snapshots_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_subject_snapshots_instance",
            doc! { "approval_process_instance_id": 1 },
        ),
        named_index(
            "idx_approval_subject_snapshots_object",
            doc! {
                "document_type": 1,
                "business_object_id": 1,
                "subject_version": 1,
            },
        ),
    ]
}

fn outbox_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_notification_outbox_id", doc! { "id": 1 }),
        unique_index("uk_approval_notification_outbox_dedup", doc! { "dedup_key": 1 }),
        named_index(
            "idx_approval_notification_outbox_delivery",
            doc! { "delivery_status": 1, "next_attempt_at": 1 },
        ),
        named_index(
            "idx_approval_notification_outbox_lease",
            doc! { "lease_until": 1, "delivery_status": 1 },
        ),
        named_index(
            "idx_approval_notification_outbox_dead_letter",
            doc! { "delivery_status": 1, "dead_lettered_at": -1 },
        ),
    ]
}

fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{outbox_indexes, snapshot_indexes};

    #[test]
    fn snapshot_indexes_are_unique_on_instance_and_query_object() {
        let indexes = snapshot_indexes();
        let instance = index_named(&indexes, "uk_approval_subject_snapshots_instance");
        assert_eq!(instance.keys, doc! { "approval_process_instance_id": 1 });
        assert_eq!(instance.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            index_named(&indexes, "idx_approval_subject_snapshots_object").keys,
            doc! {
                "document_type": 1,
                "business_object_id": 1,
                "subject_version": 1,
            }
        );
    }

    #[test]
    fn outbox_indexes_cover_dedup_lease_and_dead_letter() {
        let indexes = outbox_indexes();
        assert_eq!(
            index_named(&indexes, "uk_approval_notification_outbox_dedup").keys,
            doc! { "dedup_key": 1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_notification_outbox_delivery").keys,
            doc! { "delivery_status": 1, "next_attempt_at": 1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_notification_outbox_lease").keys,
            doc! { "lease_until": 1, "delivery_status": 1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_notification_outbox_dead_letter").keys,
            doc! { "delivery_status": 1, "dead_lettered_at": -1 }
        );
    }

    fn index_named<'a>(indexes: &'a [mongodb::IndexModel], name: &str) -> &'a mongodb::IndexModel {
        indexes
            .iter()
            .find(|index| index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name))
            .unwrap_or_else(|| panic!("missing index {name}"))
    }
}
