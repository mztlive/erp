//! 域 D03 `work_item` 的开放唯一性、审批执行关联与统一工作台索引。

use futures_util::TryStreamExt;

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
    reconcile_open_object_type_index(db).await?;
    db.collection::<Document>(WORK_ITEMS)
        .create_indexes(work_item_indexes())
        .await?;
    Ok(())
}

/// 将开放任务唯一索引升级为包含采购责任键的当前契约。
///
/// # 参数
/// * `db` - MongoDB 数据库
///
/// # 返回
/// 既有索引不存在或已符合当前契约时直接返回；否则删除旧同名索引。
///
/// # 错误
/// 集合或索引读取、删除失败时返回错误。
///
/// # 关键业务约束
/// 旧索引不含 `responsibility_key`，会错误阻止同一销售单按多个采购负责人创建任务。
async fn reconcile_open_object_type_index(db: &Database) -> Result<()> {
    let collection_names = db.list_collection_names().await?;
    if !collection_names.iter().any(|name| name == WORK_ITEMS) {
        return Ok(());
    }
    let collection = db.collection::<Document>(WORK_ITEMS);
    let mut indexes = collection.list_indexes().await?;
    while let Some(index) = indexes.try_next().await? {
        let name = index.options.as_ref().and_then(|options| options.name.as_deref());
        if name == Some("uk_work_items_open_object_type") && !is_current_open_object_type_index(&index) {
            collection.drop_index("uk_work_items_open_object_type").await?;
            break;
        }
    }
    Ok(())
}

/// 判断开放任务唯一索引是否符合当前责任范围契约。
///
/// # 参数
/// * `index` - MongoDB 已存在索引
///
/// # 返回
/// 键、唯一性和部分过滤条件全部一致时返回 `true`。
fn is_current_open_object_type_index(index: &IndexModel) -> bool {
    let Some(options) = index.options.as_ref() else {
        return false;
    };
    index.keys
        == doc! {
            "business_object_type": 1,
            "business_object_id": 1,
            "work_item_type": 1,
            "responsibility_key": 1,
        }
        && options.unique == Some(true)
        && options.partial_filter_expression
            == Some(doc! {
                "status": "OPEN",
                "owner_user_id": { "$type": "string" },
            })
}

fn work_item_indexes() -> Vec<IndexModel> {
    vec![
        unique_open_object_type_index(),
        unique_approval_execution_index(),
        named_index(
            "idx_work_items_mine",
            doc! { "status": 1, "owner_user_id": 1, "due_at": 1, "id": 1 },
        ),
        named_index(
            "idx_work_items_pending_approval",
            doc! { "status": 1, "owner_user_id": 1, "assigned_at": -1, "id": -1 },
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
            "idx_work_items_procurement_object_history",
            doc! {
                "business_object_type": 1,
                "business_object_id": 1,
                "work_item_type": 1,
                "updated_at": -1,
                "created_at": -1,
            },
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
                .partial_filter_expression(doc! {
                    "status": "OPEN",
                    "owner_user_id": { "$type": "string" },
                })
                .build(),
        )
        .build()
}

fn unique_approval_execution_index() -> IndexModel {
    IndexModel::builder()
        .keys(doc! { "approval_node_execution_id": 1 })
        .options(
            IndexOptions::builder()
                .name("uk_work_items_approval_execution".to_string())
                .unique(true)
                .partial_filter_expression(doc! {
                    "approval_node_execution_id": { "$type": "string" },
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

    use super::{is_current_open_object_type_index, work_item_indexes};

    #[test]
    fn open_object_uniqueness_requires_owner_and_execution_is_lifecycle_unique() {
        let indexes = work_item_indexes();
        let object = index_named(&indexes, "uk_work_items_open_object_type");
        assert!(is_current_open_object_type_index(object));
        assert_eq!(object.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            object.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! {
                "status": "OPEN",
                "owner_user_id": { "$type": "string" },
            })
        );

        let execution = index_named(&indexes, "uk_work_items_approval_execution");
        assert_eq!(execution.keys, doc! { "approval_node_execution_id": 1 });
        assert_eq!(execution.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            execution.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "approval_node_execution_id": { "$type": "string" } })
        );
        assert!(indexes.iter().all(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                != Some("uk_work_items_open_approval_step")
        }));
    }

    #[test]
    fn queue_indexes_follow_unified_workbench_and_drop_pool_filter() {
        let indexes = work_item_indexes();
        assert_eq!(
            index_named(&indexes, "idx_work_items_mine").keys,
            doc! { "status": 1, "owner_user_id": 1, "due_at": 1, "id": 1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_work_items_pending_approval").keys,
            doc! { "status": 1, "owner_user_id": 1, "assigned_at": -1, "id": -1 }
        );
        assert!(indexes.iter().all(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                != Some("idx_work_items_team_pool")
                && !index.keys.contains_key("owner_pool")
        }));
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
            index_named(&indexes, "idx_work_items_procurement_object_history").keys,
            doc! {
                "business_object_type": 1,
                "business_object_id": 1,
                "work_item_type": 1,
                "updated_at": -1,
                "created_at": -1,
            }
        );
    }

    fn index_named<'a>(indexes: &'a [mongodb::IndexModel], name: &str) -> &'a mongodb::IndexModel {
        indexes
            .iter()
            .find(|index| index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name))
            .unwrap_or_else(|| panic!("missing index {name}"))
    }
}
