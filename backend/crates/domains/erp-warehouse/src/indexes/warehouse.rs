//! 域 D11 `warehouse` 的索引声明：warehouse、warehouse_revision、warehouse_sku_policy。
//!
//! 集合名常量取 `WarehouseExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::bson::{Document, doc};
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::repository::extensions::WarehouseExt;

/// `warehouse` 集合名。
pub(crate) const WAREHOUSES: &str = <mongodb::Database as WarehouseExt>::WAREHOUSES;
/// `warehouse_revision` 集合名。
pub(crate) const WAREHOUSE_REVISIONS: &str = <mongodb::Database as WarehouseExt>::WAREHOUSE_REVISIONS;
/// `warehouse_sku_policy` 集合名。
pub(crate) const WAREHOUSE_SKU_POLICIES: &str = <mongodb::Database as WarehouseExt>::WAREHOUSE_SKU_POLICIES;

// 注册仓库、修订和 SKU 策略的命名索引。仓库代码全局唯一，软删除后仍占用；修订序号在仓库内唯一。同维度相同生效开始日由唯一索引先行拒绝，不能替代起始日不同的完整区间冲突检查。
// 同仓库修订有效期及同仓同 SKU 启用区间的完整重叠约束须在写入用例事务中核查。现存数据冲突或 MongoDB 创建失败时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    for (collection, indexes) in [
        (WAREHOUSES, warehouse_indexes()),
        (WAREHOUSE_REVISIONS, warehouse_revision_indexes()),
        (WAREHOUSE_SKU_POLICIES, warehouse_sku_policy_indexes()),
    ] {
        db.collection::<Document>(collection).create_indexes(indexes).await?;
    }
    Ok(())
}

// 定义仓库 ID 和稳定代码的全局唯一索引，以及支持启停筛选的索引。
fn warehouse_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_warehouses_id", doc! { "id": 1 }),
        unique_index("uk_warehouses_warehouse_code", doc! { "warehouse_code": 1 }),
        named_index("idx_warehouses_status", doc! { "status": 1 }),
    ]
}

// 定义修订 ID、仓库内修订序号和生效开始日的唯一约束，支持稳定定位历史快照。
fn warehouse_revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_warehouse_revisions_id", doc! { "id": 1 }),
        unique_index("uk_warehouse_revisions_revision", doc! { "warehouse_id": 1, "revision_no": 1 }),
        // 「同一仓库有效期不得重叠」的保守子集（§6.3）：同一仓库生效开始日
        // 相同必然重叠，直接拒绝；完整区间重叠校验在 P3 Service 事务内完成。
        unique_index(
            "uk_warehouse_revisions_effective_from",
            doc! { "warehouse_id": 1, "effective_from": 1 },
        ),
    ]
}

// 定义策略身份和同仓同 SKU 相同开始日的唯一约束，并建立按维度、区间及启停查询的索引。
fn warehouse_sku_policy_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_warehouse_sku_policies_id", doc! { "id": 1 }),
        // 「同一仓库和 SKU 的启用区间不得重叠」的保守子集（§6.3）：同仓库同
        // SKU 生效开始日相同必然重叠，直接拒绝；完整区间重叠校验在 P3
        // Service 事务内以范围查询完成。
        unique_index(
            "uk_warehouse_sku_policies_start",
            doc! { "warehouse_id": 1, "sku_id": 1, "effective_from": 1 },
        ),
        named_index(
            "idx_warehouse_sku_policies_lookup",
            doc! {
                "warehouse_id": 1,
                "sku_id": 1,
                "effective_from": 1,
                "effective_to": 1,
                "status": 1,
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    // 验证三个集合均有 `id` 唯一索引，使稳定身份查询及乐观锁更新不用扫描集合。
    #[test]
    fn collections_look_up_by_unique_stable_id() {
        for indexes in [
            super::warehouse_indexes(),
            super::warehouse_revision_indexes(),
            super::warehouse_sku_policy_indexes(),
        ] {
            assert!(indexes.iter().any(|index| {
                index.keys == mongodb::bson::doc! { "id": 1 }
                    && index.options.as_ref().and_then(|options| options.unique) == Some(true)
            }));
        }
    }

    use mongodb::bson::doc;

    use super::{warehouse_indexes, warehouse_revision_indexes, warehouse_sku_policy_indexes};

    fn names(indexes: &[mongodb::IndexModel]) -> Vec<String> {
        indexes.iter().filter_map(|index| index.options.as_ref()?.name.clone()).collect()
    }

    #[test]
    fn warehouse_code_identity_index_is_globally_unique() {
        let indexes = warehouse_indexes();

        let code = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_warehouses_warehouse_code")
            })
            .unwrap();
        assert_eq!(code.keys, doc! { "warehouse_code": 1 });
        assert_eq!(code.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn warehouse_revision_indexes_cover_revision_and_effective_surface() {
        let indexes = warehouse_revision_indexes();
        let names = names(&indexes);

        assert!(names.contains(&"uk_warehouse_revisions_revision".to_string()));
        assert!(names.contains(&"uk_warehouse_revisions_effective_from".to_string()));
        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "warehouse_id": 1, "effective_from": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }

    #[test]
    fn warehouse_sku_policy_indexes_cover_start_surface_and_lookup() {
        let indexes = warehouse_sku_policy_indexes();
        let names = names(&indexes);

        assert!(names.contains(&"uk_warehouse_sku_policies_start".to_string()));
        assert!(indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "warehouse_id": 1,
                    "sku_id": 1,
                    "effective_from": 1,
                    "effective_to": 1,
                    "status": 1,
                }
        }));
    }
}
