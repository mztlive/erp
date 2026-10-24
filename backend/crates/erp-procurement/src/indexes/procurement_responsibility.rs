//! 采购责任规则唯一性与管理查询索引。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::repository::extensions::ProcurementResponsibilityExt;
use crate::repository::procurement_responsibility::migration::migrate_pools;

/// 创建采购责任规则索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 返回
/// 全部命名索引创建成功返回 `Ok(())`。
///
/// # 错误
/// 已有启用规则违反选择器唯一性或 MongoDB 创建失败时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    migrate_pools(db).await?;
    db.collection::<Document>(<Database as ProcurementResponsibilityExt>::PROCUREMENT_RESPONSIBILITY_RULES)
        .create_indexes(indexes())
        .await?;
    Ok(())
}

/// 构造采购责任规则索引集合。
///
/// # 返回
/// 返回启用选择器唯一索引及列表、负责人查询索引。
fn indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_procurement_responsibility_rules_id", doc! { "id": 1 }),
        IndexModel::builder()
            .keys(doc! { "selector_key": 1 })
            .options(
                IndexOptions::builder()
                    .name("uk_procurement_responsibility_active_selector".to_string())
                    .unique(true)
                    .partial_filter_expression(doc! {
                        "status": "active",
                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                    })
                    .build(),
            )
            .build(),
        named_index(
            "idx_procurement_responsibility_list",
            doc! { "deleted_at": 1, "status": 1, "rule_type": 1, "created_at": 1 },
        ),
        named_index(
            "idx_procurement_responsibility_pool_owner",
            doc! { "deleted_at": 1, "owner_user_ids": 1, "status": 1 },
        ),
    ]
}

#[cfg(test)]
mod tests {
    /// 按稳定主键查询与乐观锁更新依赖 `id` 唯一索引，否则退化为集合扫描。
    #[test]
    fn collections_look_up_by_unique_stable_id() {
        let indexes = super::indexes();
        assert!(indexes.iter().any(|index| {
            index.keys == mongodb::bson::doc! { "id": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }

    use entity_core::NOT_DELETED_TIMESTAMP_BSON;
    use mongodb::bson::doc;

    use super::indexes;

    #[test]
    fn active_selector_is_uniquely_constrained() {
        let index = indexes()
            .into_iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_procurement_responsibility_active_selector")
            })
            .expect("唯一索引存在");
        let options = index.options.expect("索引选项存在");
        assert_eq!(index.keys, doc! { "selector_key": 1 });
        assert_eq!(options.unique, Some(true));
        assert_eq!(
            options.partial_filter_expression,
            Some(doc! {
                "status": "active",
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            })
        );
    }
}
