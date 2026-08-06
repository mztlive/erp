//! 域 D20 `cost` 的索引声明：cost_entry、cost_allocation。
//!
//! 集合名常量取 `CostExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::CostExt;
use crate::Result;

/// `cost_entry` 集合名。
pub(crate) const COST_ENTRIES: &str = <mongodb::Database as CostExt>::COST_ENTRIES;
/// `cost_allocation` 集合名。
pub(crate) const COST_ALLOCATIONS: &str = <mongodb::Database as CostExt>::COST_ALLOCATIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.10「必需约束与索引」。成本事实是正式事实集合
/// （§4.5 不设业务软删除，冲减用 `REDUCTION` 阶段追加事实），业务幂等唯一
/// 用唯一索引保证，不提供软删除方法。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, COST_ENTRIES, cost_entry_indexes()).await?;
    create_indexes(db, COST_ALLOCATIONS, cost_allocation_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `cost_entry` 的业务幂等与利润查询索引。
fn cost_entry_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_cost_entries_identity",
            doc! {
                "source_fact_type": 1,
                "source_document_id": 1,
                "source_line_id": 1,
                "source_version": 1,
                "cost_stage": 1,
                "cost_type": 1,
            },
        ),
        named_index(
            "idx_cost_entries_profit",
            doc! { "cost_scope": 1, "cost_stage": 1 },
        ),
        named_index(
            "idx_cost_entries_stage_time",
            doc! { "cost_stage": 1, "occurred_at": 1 },
        ),
        named_index(
            "idx_cost_entries_supplier",
            doc! { "supplier_id": 1, "cost_stage": 1 },
        ),
    ]
}

/// 返回 `cost_allocation` 的经营归属与消费归集查询索引。
fn cost_allocation_indexes() -> Vec<IndexModel> {
    vec![
        named_index("idx_cost_allocations_entry", doc! { "cost_entry_id": 1 }),
        named_index(
            "idx_cost_allocations_sales_order",
            doc! { "sales_order_id": 1, "sales_order_line_id": 1 },
        ),
        named_index(
            "idx_cost_allocations_consumption",
            doc! { "mall_consumption_entry_id": 1 },
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
    use mongodb::IndexModel;

    use super::{cost_allocation_indexes, cost_entry_indexes};

    fn name(index: &IndexModel) -> Option<&str> {
        index.options.as_ref().and_then(|options| options.name.as_deref())
    }

    #[test]
    fn cost_entry_identity_covers_the_full_business_key() {
        let indexes = cost_entry_indexes();

        let identity = indexes
            .iter()
            .find(|index| name(index) == Some("uk_cost_entries_identity"))
            .unwrap();
        assert_eq!(
            identity.keys,
            doc! {
                "source_fact_type": 1,
                "source_document_id": 1,
                "source_line_id": 1,
                "source_version": 1,
                "cost_stage": 1,
                "cost_type": 1,
            }
        );
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| name(index) == Some("idx_cost_entries_profit")));
        assert!(indexes
            .iter()
            .any(|index| name(index) == Some("idx_cost_entries_stage_time")));
    }

    #[test]
    fn cost_allocation_indexes_cover_ownership_and_consumption_lookups() {
        let indexes = cost_allocation_indexes();

        assert!(indexes
            .iter()
            .any(|index| name(index) == Some("idx_cost_allocations_entry")));
        assert!(indexes.iter().any(|index| {
            name(index) == Some("idx_cost_allocations_sales_order")
                && index.keys == doc! { "sales_order_id": 1, "sales_order_line_id": 1 }
        }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "mall_consumption_entry_id": 1 }));
    }
}
