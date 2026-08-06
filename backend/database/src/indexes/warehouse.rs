//! 域 D11 `warehouse` 的索引声明：warehouse、warehouse_revision、warehouse_sku_policy。
//!
//! 集合名常量取 `WarehouseExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::WarehouseExt;
use crate::Result;

/// `warehouse` 集合名。
pub(crate) const WAREHOUSES: &str = <mongodb::Database as WarehouseExt>::WAREHOUSES;
/// `warehouse_revision` 集合名。
pub(crate) const WAREHOUSE_REVISIONS: &str = <mongodb::Database as WarehouseExt>::WAREHOUSE_REVISIONS;
/// `warehouse_sku_policy` 集合名。
pub(crate) const WAREHOUSE_SKU_POLICIES: &str = <mongodb::Database as WarehouseExt>::WAREHOUSE_SKU_POLICIES;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.3「必需约束与索引」：`warehouse_code` 使用**全局唯一
/// 索引**（与 accounts 的 code 处理一致）：软删除后仍保留身份，避免复用破坏
/// 恢复语义；修订表聚合内 `(warehouse_id, revision_no)` 唯一。
///
/// 「同一仓库有效期不得重叠」「同一仓库和 SKU 的启用区间不得重叠」无法由
/// MongoDB 普通唯一索引完整表达（重叠窗口起止日不同）；这里以「生效开始日
/// 相同 = 必然重叠」的保守子集落地唯一索引（`uk_*_effective_from`），
/// 非法组合在写路径即被拒绝；完整区间重叠校验由 P3 Service 在事务内以
/// 范围查询完成，两者共同覆盖 §6.3 约束。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, WAREHOUSES, warehouse_indexes()).await?;
    create_indexes(db, WAREHOUSE_REVISIONS, warehouse_revision_indexes()).await?;
    create_indexes(db, WAREHOUSE_SKU_POLICIES, warehouse_sku_policy_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `warehouse` 的身份约束与启停查询索引。
fn warehouse_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_warehouses_warehouse_code", doc! { "warehouse_code": 1 }),
        named_index("idx_warehouses_status", doc! { "status": 1 }),
    ]
}

/// 返回 `warehouse_revision` 的聚合修订唯一约束与查询索引。
fn warehouse_revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_warehouse_revisions_revision",
            doc! { "warehouse_id": 1, "revision_no": 1 },
        ),
        // 「同一仓库有效期不得重叠」的保守子集（§6.3）：同一仓库生效开始日
        // 相同必然重叠，直接拒绝；完整区间重叠校验在 P3 Service 事务内完成。
        unique_index(
            "uk_warehouse_revisions_effective_from",
            doc! { "warehouse_id": 1, "effective_from": 1 },
        ),
    ]
}

/// 返回 `warehouse_sku_policy` 的启用区间唯一子集与查询索引。
fn warehouse_sku_policy_indexes() -> Vec<IndexModel> {
    vec![
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

/// 构建命名普通索引。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

/// 构建命名唯一索引。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{warehouse_indexes, warehouse_revision_indexes, warehouse_sku_policy_indexes};

    fn names(indexes: &[mongodb::IndexModel]) -> Vec<String> {
        indexes
            .iter()
            .filter_map(|index| index.options.as_ref()?.name.clone())
            .collect()
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
