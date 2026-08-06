//! 域 D03 `work_item` 的索引声明：work_item。
//!
//! 集合名常量取 `WorkItemExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::WorkItemExt;
use crate::Result;

/// `work_item` 集合名。
pub(crate) const WORK_ITEMS: &str = <mongodb::Database as WorkItemExt>::WORK_ITEMS;

/// 创建本域集合的幂等命名索引。
///
/// 落地数据模型 §6.1「必需约束与索引」：
/// - 同一业务对象、任务类型同时最多一个有效任务：用**部分唯一索引**
///   `(business_object_type, business_object_id, work_item_type)`（仅约束
///   `UNCLAIMED` / `IN_PROGRESS` 两个有效态）。理由与回滚方式见
///   `work_item_indexes` 的注释；
/// - `owner_role + owner_user_id + status + due_at` 工作队列索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, WORK_ITEMS, work_item_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `work_item` 的有效任务唯一约束与工作队列索引。
///
/// 「同一业务对象、任务类型同时最多一个有效任务」（§6.1）使用**部分唯一索引**
/// 落地：`COMPLETED` / `CLOSED` 是终态历史，同一对象允许在历史任务完成/关闭后
/// 重新派发新任务（如重复确认轮次），因此唯一性只约束 `UNCLAIMED` /
/// `IN_PROGRESS` 两个有效态，不能做成全局唯一。`partialFilterExpression` 只支持
/// `$or` + `$eq` 组合（不支持 `$in`），故显式列出两个状态。
///
/// 回滚方式：若业务规则收紧为「同一对象一生只有一个任务」，删除本部分唯一索引，
/// 改为全局唯一索引 `uk_work_items_object_type`，并清理历史重复数据。
fn work_item_indexes() -> Vec<IndexModel> {
    vec![
        IndexModel::builder()
            .keys(doc! {
                "business_object_type": 1,
                "business_object_id": 1,
                "work_item_type": 1,
            })
            .options(
                IndexOptions::builder()
                    .name("uk_work_items_active".to_string())
                    .unique(true)
                    .partial_filter_expression(doc! {
                        "$or": [
                            { "status": "UNCLAIMED" },
                            { "status": "IN_PROGRESS" },
                        ]
                    })
                    .build(),
            )
            .build(),
        named_index(
            "idx_work_items_queue",
            doc! {
                "owner_role": 1,
                "owner_user_id": 1,
                "status": 1,
                "due_at": 1,
            },
        ),
    ]
}

/// 构建命名普通索引。
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
    fn active_task_uniqueness_is_partial_over_non_terminal_states() {
        let indexes = work_item_indexes();

        let active = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_work_items_active")
            })
            .unwrap();
        assert_eq!(active.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            active.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! {
                "$or": [
                    { "status": "UNCLAIMED" },
                    { "status": "IN_PROGRESS" },
                ]
            })
        );
        assert_eq!(
            active.keys,
            doc! {
                "business_object_type": 1,
                "business_object_id": 1,
                "work_item_type": 1,
            }
        );
    }

    #[test]
    fn work_queue_index_follows_document_field_order() {
        let indexes = work_item_indexes();

        assert!(indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "owner_role": 1,
                    "owner_user_id": 1,
                    "status": 1,
                    "due_at": 1,
                }
        }));
    }
}
