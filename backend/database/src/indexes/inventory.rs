//! 域 D17 `inventory` 的索引声明：stock_movement、stock_balance、
//! stock_reservation(+_entry)、stock_adjustment(+_line)。
//!
//! 集合名常量取 `InventoryExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。
//!
//! 逐条落地数据模型 §6.7「必需约束与索引」：
//! - `stock_movement` 是正式事实（§4.5.1）：`(source_document_id,
//!   source_line_id, movement_type)` 唯一索引直接落地「对同一业务动作唯一」，
//!   禁止重复入账；`source_line_id` 为可空，缺失时按 null 参与唯一判定，
//!   同一业务动作必须携带行级来源（实体构造已要求来源单据标识非空）；
//! - `stock_balance` 的 `(warehouse_id, sku_id)` 是全局唯一库存维度（§6.7）；
//! - `stock_reservation` 的建立动作唯一（合格入库来源 + 采购分配，§6.7）；
//! - 事实类集合（movement/entry）不设业务软删除，无需部分唯一索引。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::InventoryExt;
use crate::Result;

/// `stock_movement` 集合名。
pub(crate) const STOCK_MOVEMENTS: &str = <mongodb::Database as InventoryExt>::STOCK_MOVEMENTS;
/// `stock_balance` 集合名。
pub(crate) const STOCK_BALANCES: &str = <mongodb::Database as InventoryExt>::STOCK_BALANCES;
/// `stock_reservation` 集合名。
pub(crate) const STOCK_RESERVATIONS: &str = <mongodb::Database as InventoryExt>::STOCK_RESERVATIONS;
/// `stock_reservation_entry` 集合名。
pub(crate) const STOCK_RESERVATION_ENTRIES: &str =
    <mongodb::Database as InventoryExt>::STOCK_RESERVATION_ENTRIES;
/// `stock_adjustment` 集合名。
pub(crate) const STOCK_ADJUSTMENTS: &str = <mongodb::Database as InventoryExt>::STOCK_ADJUSTMENTS;
/// `stock_adjustment_line` 集合名。
pub(crate) const STOCK_ADJUSTMENT_LINES: &str = <mongodb::Database as InventoryExt>::STOCK_ADJUSTMENT_LINES;

/// 创建本域集合的幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, STOCK_MOVEMENTS, stock_movement_indexes()).await?;
    create_indexes(db, STOCK_BALANCES, stock_balance_indexes()).await?;
    create_indexes(db, STOCK_RESERVATIONS, stock_reservation_indexes()).await?;
    create_indexes(db, STOCK_RESERVATION_ENTRIES, stock_reservation_entry_indexes()).await?;
    create_indexes(db, STOCK_ADJUSTMENTS, stock_adjustment_indexes()).await?;
    create_indexes(db, STOCK_ADJUSTMENT_LINES, stock_adjustment_line_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `stock_movement` 的来源去重与台账查询索引（§6.7）。
fn stock_movement_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_stock_movements_source",
            doc! { "source_document_id": 1, "source_line_id": 1, "movement_type": 1 },
        ),
        named_index(
            "idx_stock_movements_ledger",
            doc! { "warehouse_id": 1, "sku_id": 1, "occurred_at": 1, "id": 1 },
        ),
    ]
}

/// 返回 `stock_balance` 的库存维度唯一约束（§6.7）。
fn stock_balance_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_stock_balances_dimension",
        doc! { "warehouse_id": 1, "sku_id": 1 },
    )]
}

/// 返回 `stock_reservation` 的建立动作唯一与预占查询索引（§6.7）。
fn stock_reservation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_stock_reservations_establish",
            doc! { "source_receipt_line_id": 1, "purchase_line_sales_allocation_id": 1 },
        ),
        named_index(
            "idx_stock_reservations_warehouse_sku_status",
            doc! { "warehouse_id": 1, "sku_id": 1, "status": 1 },
        ),
        named_index(
            "idx_stock_reservations_sales_line_status",
            doc! { "sales_order_line_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `stock_reservation_entry` 的预占流水查询索引（§6.7）。
fn stock_reservation_entry_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_stock_reservation_entries_reservation",
        doc! { "reservation_id": 1, "entry_type": 1 },
    )]
}

/// 返回 `stock_adjustment` 的身份约束与列表查询索引（§6.7）。
fn stock_adjustment_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_stock_adjustments_adjustment_no", doc! { "adjustment_no": 1 }),
        named_index(
            "idx_stock_adjustments_warehouse_status",
            doc! { "warehouse_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `stock_adjustment_line` 的明细查询索引（§6.7）。
fn stock_adjustment_line_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_stock_adjustment_lines_adjustment",
        doc! { "stock_adjustment_id": 1 },
    )]
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

    use super::{
        stock_adjustment_indexes, stock_adjustment_line_indexes, stock_balance_indexes,
        stock_movement_indexes, stock_reservation_entry_indexes, stock_reservation_indexes,
    };

    #[test]
    fn stock_movement_source_index_is_unique_and_ledger_index_present() {
        let indexes = stock_movement_indexes();
        let source = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_stock_movements_source")
            })
            .unwrap();
        assert_eq!(
            source.keys,
            doc! { "source_document_id": 1, "source_line_id": 1, "movement_type": 1 }
        );
        assert_eq!(source.options.as_ref().unwrap().unique, Some(true));
        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "warehouse_id": 1, "sku_id": 1, "occurred_at": 1, "id": 1 }
        }));
    }

    #[test]
    fn stock_balance_dimension_is_globally_unique() {
        let indexes = stock_balance_indexes();
        let dimension = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_stock_balances_dimension")
            })
            .unwrap();
        assert_eq!(dimension.keys, doc! { "warehouse_id": 1, "sku_id": 1 });
        assert_eq!(dimension.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn stock_reservation_indexes_cover_establish_uniqueness_and_queries() {
        let indexes = stock_reservation_indexes();
        let establish = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_stock_reservations_establish")
            })
            .unwrap();
        assert_eq!(
            establish.keys,
            doc! { "source_receipt_line_id": 1, "purchase_line_sales_allocation_id": 1 }
        );
        assert_eq!(establish.options.as_ref().unwrap().unique, Some(true));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "warehouse_id": 1, "sku_id": 1, "status": 1 } }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "sales_order_line_id": 1, "status": 1 } }));
    }

    #[test]
    fn entry_and_adjustment_indexes_cover_queries() {
        let entry = stock_reservation_entry_indexes();
        assert!(entry
            .iter()
            .any(|index| index.keys == doc! { "reservation_id": 1, "entry_type": 1 }));

        let adjustment = stock_adjustment_indexes();
        let identity = adjustment
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_stock_adjustments_adjustment_no")
            })
            .unwrap();
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));
        assert!(adjustment
            .iter()
            .any(|index| index.keys == doc! { "warehouse_id": 1, "status": 1 }));

        assert!(stock_adjustment_line_indexes()
            .iter()
            .any(|index| index.keys == doc! { "stock_adjustment_id": 1 }));
    }
}
