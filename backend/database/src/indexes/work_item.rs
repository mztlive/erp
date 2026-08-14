//! 域 D03 `work_item` 的开放唯一性与责任队列索引。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::WorkItemExt;
use crate::Result;

/// `work_items` 集合名。
pub(crate) const WORK_ITEMS: &str = <mongodb::Database as WorkItemExt>::WORK_ITEMS;

/// 幂等创建任务责任合同要求的命名索引。
///
/// # 错误
/// 既有数据违反开放唯一性，或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(WORK_ITEMS)
        .create_indexes(work_item_indexes())
        .await?;
    Ok(())
}

fn work_item_indexes() -> Vec<IndexModel> {
    vec![
        unique_open_object_type_index(),
        unique_open_approval_step_index(),
        named_index(
            "idx_work_items_mine",
            doc! { "status": 1, "owner_user_id": 1, "due_at": 1 },
        ),
        named_index(
            "idx_work_items_team_pool",
            doc! {
                "status": 1,
                "assignment_mode": 1,
                "owner_role": 1,
                "due_at": 1,
            },
        ),
        named_index(
            "idx_work_items_managed",
            doc! {
                "status": 1,
                "owner_organization_id": 1,
                "owner_user_id": 1,
                "due_at": 1,
            },
        ),
        named_index(
            "idx_work_items_responsibility_history",
            doc! { "status": 1, "responsibility_actor_ids": 1, "due_at": 1 },
        ),
        named_index(
            "idx_work_items_completed_history",
            doc! { "status": 1, "completed_by": 1, "completed_at": -1 },
        ),
        named_index(
            "idx_work_items_closed_history",
            doc! { "status": 1, "closed_by": 1, "closed_at": -1 },
        ),
    ]
}

fn unique_open_object_type_index() -> IndexModel {
    IndexModel::builder()
        .keys(doc! {
            "business_object_type": 1,
            "business_object_id": 1,
            "work_item_type": 1,
            "responsibility_key": 1,
        })
        .options(
            IndexOptions::builder()
                .name("uk_work_items_open_object_type".to_string())
                .unique(true)
                .partial_filter_expression(doc! { "status": "OPEN" })
                .build(),
        )
        .build()
}

fn unique_open_approval_step_index() -> IndexModel {
    IndexModel::builder()
        .keys(doc! { "approval_step_instance_id": 1 })
        .options(
            IndexOptions::builder()
                .name("uk_work_items_open_approval_step".to_string())
                .unique(true)
                .partial_filter_expression(doc! {
                    "status": "OPEN",
                    "approval_step_instance_id": { "$type": "string" },
                })
                .build(),
        )
        .build()
}

fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::work_item_indexes;

    #[test]
    fn open_uniqueness_excludes_terminal_history() {
        let indexes = work_item_indexes();
        let object = index_named(&indexes, "uk_work_items_open_object_type");
        assert_eq!(object.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            object.keys,
            doc! {
                "business_object_type": 1,
                "business_object_id": 1,
                "work_item_type": 1,
                "responsibility_key": 1,
            }
        );
        assert_eq!(
            object.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "status": "OPEN" })
        );

        let step = index_named(&indexes, "uk_work_items_open_approval_step");
        assert_eq!(step.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            step.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! {
                "status": "OPEN",
                "approval_step_instance_id": { "$type": "string" },
            })
        );
    }

    #[test]
    fn queue_indexes_follow_contract_field_order() {
        let indexes = work_item_indexes();

        assert_eq!(
            index_named(&indexes, "idx_work_items_mine").keys,
            doc! { "status": 1, "owner_user_id": 1, "due_at": 1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_work_items_team_pool").keys,
            doc! {
                "status": 1,
                "assignment_mode": 1,
                "owner_role": 1,
                "due_at": 1,
            }
        );
        assert_eq!(
            index_named(&indexes, "idx_work_items_managed").keys,
            doc! {
                "status": 1,
                "owner_organization_id": 1,
                "owner_user_id": 1,
                "due_at": 1,
            }
        );
        assert_eq!(
            index_named(&indexes, "idx_work_items_responsibility_history").keys,
            doc! { "status": 1, "responsibility_actor_ids": 1, "due_at": 1 }
        );
    }

    fn index_named<'a>(indexes: &'a [mongodb::IndexModel], name: &str) -> &'a mongodb::IndexModel {
        indexes
            .iter()
            .find(|index| index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name))
            .unwrap()
    }
}
