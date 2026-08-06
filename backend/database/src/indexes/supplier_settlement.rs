//! 域 D33 `supplier_settlement` 的索引声明：supplier_settlement_statement、
//! supplier_settlement_item、supplier_settlement_difference。
//!
//! 集合名常量取 `SupplierSettlementExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SupplierSettlementExt;
use crate::Result;

/// `supplier_settlement_statement` 集合名。
pub(crate) const SUPPLIER_SETTLEMENT_STATEMENTS: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS;
/// `supplier_settlement_item` 集合名。
pub(crate) const SUPPLIER_SETTLEMENT_ITEMS: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_ITEMS;
/// `supplier_settlement_difference` 集合名。
pub(crate) const SUPPLIER_SETTLEMENT_DIFFERENCES: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCES;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.20「必需约束与索引」。结算单号使用**全局唯一索引**
/// （与 accounts 的 code 处理一致）：软删除后仍保留身份，避免复用破坏恢复语义；
/// 可空外部账单身份与「已确认不重复覆盖」使用**部分唯一索引**（仅满足条件的
/// 文档参与唯一约束），保证 `NULL` 不阻塞多条未填写记录的写入。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(
        db,
        SUPPLIER_SETTLEMENT_STATEMENTS,
        supplier_settlement_statement_indexes(),
    )
    .await?;
    create_indexes(db, SUPPLIER_SETTLEMENT_ITEMS, supplier_settlement_item_indexes()).await?;
    create_indexes(
        db,
        SUPPLIER_SETTLEMENT_DIFFERENCES,
        supplier_settlement_difference_indexes(),
    )
    .await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `supplier_settlement_statement` 的身份约束和查询索引（§6.20）。
fn supplier_settlement_statement_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_settlement_statements_statement_no",
            doc! { "statement_no": 1 },
        ),
        // §6.20「有外部账单时 (supplier_id, external_bill_no, external_bill_version) 唯一」：
        // 部分唯一索引，外部账单身份为空（纯 ERP 结算单）时多条记录不受约束；
        // 回滚方式：删除本索引后以唯一约束下放到应用层校验。
        partial_unique_index(
            "uk_supplier_settlement_statements_supplier_external_bill",
            doc! {
                "supplier_id": 1,
                "external_bill_no": 1,
                "external_bill_version": 1,
            },
            doc! { "external_bill_no": { "$type": "string" } },
        ),
        // §6.20「同一供应商同一结算范围不得被两个已确认结算单重复覆盖」：
        // 部分唯一索引，仅已确认结算单的 (supplier_id, period_start, period_end)
        // 参与唯一约束，草稿/待对账等中间状态不阻塞同范围结算单的迭代；
        // 回滚方式：删除本索引后以 P3 确认事务校验替代。
        partial_unique_index(
            "uk_supplier_settlement_statements_supplier_period_confirmed",
            doc! { "supplier_id": 1, "period_start": 1, "period_end": 1 },
            doc! { "status": { "$eq": "CONFIRMED" } },
        ),
        named_index(
            "idx_supplier_settlement_statements_supplier_period_status",
            doc! { "supplier_id": 1, "period_start": 1, "period_end": 1, "status": 1 },
        ),
    ]
}

/// 返回 `supplier_settlement_item` 的身份约束索引（§6.20）。
fn supplier_settlement_item_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_settlement_items_statement_fulfillment_item",
        doc! { "statement_id": 1, "supplier_fulfillment_item_id": 1 },
    )]
}

/// 返回 `supplier_settlement_difference` 的查询索引（§6.20）。
fn supplier_settlement_difference_indexes() -> Vec<IndexModel> {
    vec![
        named_index(
            "idx_supplier_settlement_differences_statement_item",
            doc! { "statement_item_id": 1 },
        ),
        named_index("idx_supplier_settlement_differences_status", doc! { "status": 1 }),
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

/// 构建命名部分唯一索引（仅匹配部分筛选表达式的文档参与唯一约束）。
fn partial_unique_index(name: impl Into<String>, keys: Document, filter: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder()
                .name(name.into())
                .unique(true)
                .partial_filter_expression(filter)
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        supplier_settlement_difference_indexes, supplier_settlement_item_indexes,
        supplier_settlement_statement_indexes,
    };

    #[test]
    fn statement_identity_and_query_indexes_match_section_6_20() {
        let indexes = supplier_settlement_statement_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_settlement_statements_statement_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        let external_bill = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_settlement_statements_supplier_external_bill")
            })
            .unwrap();
        assert_eq!(external_bill.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            external_bill.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "external_bill_no": { "$type": "string" } })
        );
        let confirmed = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_settlement_statements_supplier_period_confirmed")
            })
            .unwrap();
        assert_eq!(confirmed.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            confirmed.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "status": { "$eq": "CONFIRMED" } })
        );
        assert!(indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "supplier_id": 1,
                    "period_start": 1,
                    "period_end": 1,
                    "status": 1,
                }
        }));
    }

    #[test]
    fn settlement_item_identity_is_unique() {
        let indexes = supplier_settlement_item_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_settlement_items_statement_fulfillment_item")
            })
            .unwrap();
        assert_eq!(
            identity.keys,
            doc! { "statement_id": 1, "supplier_fulfillment_item_id": 1 }
        );
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn difference_query_indexes_cover_item_and_status() {
        let indexes = supplier_settlement_difference_indexes();

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "statement_item_id": 1 }));
        assert!(indexes.iter().any(|index| index.keys == doc! { "status": 1 }));
    }
}
