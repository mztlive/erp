//! 域 D21 `returns` 的索引声明：sales_return_case、sales_return_line、
//! purchase_return_order、purchase_return_line、customer_refund、supplier_refund、
//! receipt_reversal、payment_reversal。
//!
//! 集合名常量取 `ReturnsExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::ReturnsExt;
use crate::Result;

/// `sales_return_case` 集合名。
pub(crate) const SALES_RETURN_CASES: &str = <mongodb::Database as ReturnsExt>::SALES_RETURN_CASES;
/// `sales_return_line` 集合名。
pub(crate) const SALES_RETURN_LINES: &str = <mongodb::Database as ReturnsExt>::SALES_RETURN_LINES;
/// `purchase_return_order` 集合名。
pub(crate) const PURCHASE_RETURN_ORDERS: &str = <mongodb::Database as ReturnsExt>::PURCHASE_RETURN_ORDERS;
/// `purchase_return_line` 集合名。
pub(crate) const PURCHASE_RETURN_LINES: &str = <mongodb::Database as ReturnsExt>::PURCHASE_RETURN_LINES;
/// `customer_refund` 集合名。
pub(crate) const CUSTOMER_REFUNDS: &str = <mongodb::Database as ReturnsExt>::CUSTOMER_REFUNDS;
/// `supplier_refund` 集合名。
pub(crate) const SUPPLIER_REFUNDS: &str = <mongodb::Database as ReturnsExt>::SUPPLIER_REFUNDS;
/// `receipt_reversal` 集合名。
pub(crate) const RECEIPT_REVERSALS: &str = <mongodb::Database as ReturnsExt>::RECEIPT_REVERSALS;
/// `payment_reversal` 集合名。
pub(crate) const PAYMENT_REVERSALS: &str = <mongodb::Database as ReturnsExt>::PAYMENT_REVERSALS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.11「必需约束与索引」。本域全部集合是退货/退款/冲正
/// 事实与处理单（§4.5 不设业务软删除，纠错用反向事实），**不提供软删除
/// 方法**；处理单编号用**全局唯一索引**（与 accounts 的 code 处理一致），
/// 软删除语义不适用但仍保留身份唯一，避免作废后复用破坏追溯。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SALES_RETURN_CASES, sales_return_case_indexes()).await?;
    create_indexes(db, SALES_RETURN_LINES, sales_return_line_indexes()).await?;
    create_indexes(db, PURCHASE_RETURN_ORDERS, purchase_return_order_indexes()).await?;
    create_indexes(db, PURCHASE_RETURN_LINES, purchase_return_line_indexes()).await?;
    create_indexes(db, CUSTOMER_REFUNDS, customer_refund_indexes()).await?;
    create_indexes(db, SUPPLIER_REFUNDS, supplier_refund_indexes()).await?;
    create_indexes(db, RECEIPT_REVERSALS, receipt_reversal_indexes()).await?;
    create_indexes(db, PAYMENT_REVERSALS, payment_reversal_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `sales_return_case` 的编号唯一与处理队列索引。
fn sales_return_case_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_return_cases_no", doc! { "return_no": 1 }),
        named_index(
            "idx_sales_return_cases_order_status",
            doc! { "sales_order_id": 1, "status": 1 },
        ),
        named_index("idx_sales_return_cases_status", doc! { "status": 1 }),
    ]
}

/// 返回 `sales_return_line` 的退货单与原明细追溯索引。
fn sales_return_line_indexes() -> Vec<IndexModel> {
    vec![
        named_index("idx_sales_return_lines_case", doc! { "sales_return_case_id": 1 }),
        named_index(
            "idx_sales_return_lines_order_line",
            doc! { "sales_order_line_id": 1 },
        ),
    ]
}

/// 返回 `purchase_return_order` 的编号唯一与采购单追溯索引。
fn purchase_return_order_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_purchase_return_orders_no", doc! { "purchase_return_no": 1 }),
        named_index(
            "idx_purchase_return_orders_po_status",
            doc! { "purchase_order_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `purchase_return_line` 的退货单追溯索引。
fn purchase_return_line_indexes() -> Vec<IndexModel> {
    vec![
        named_index(
            "idx_purchase_return_lines_order",
            doc! { "purchase_return_order_id": 1 },
        ),
        named_index(
            "idx_purchase_return_lines_rev_line",
            doc! { "purchase_order_revision_line_id": 1 },
        ),
    ]
}

/// 返回 `customer_refund` 的编号唯一、列表与原事实追溯索引。
fn customer_refund_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_customer_refunds_no", doc! { "refund_no": 1 }),
        named_index(
            "idx_customer_refunds_customer_status",
            doc! { "customer_id": 1, "status": 1 },
        ),
        named_index(
            "idx_customer_refunds_original",
            doc! { "original_receipt_id": 1, "original_receivable_entry_id": 1 },
        ),
    ]
}

/// 返回 `supplier_refund` 的编号唯一、列表与原事实追溯索引。
fn supplier_refund_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_supplier_refunds_no", doc! { "refund_no": 1 }),
        named_index(
            "idx_supplier_refunds_supplier_status",
            doc! { "supplier_id": 1, "status": 1 },
        ),
        named_index(
            "idx_supplier_refunds_original",
            doc! { "original_payment_id": 1, "original_payable_entry_id": 1 },
        ),
    ]
}

/// 返回 `receipt_reversal` 的编号唯一与原回款追溯索引。
fn receipt_reversal_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_receipt_reversals_no", doc! { "reversal_no": 1 }),
        named_index(
            "idx_receipt_reversals_original",
            doc! { "original_customer_receipt_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `payment_reversal` 的编号唯一与原付款追溯索引。
fn payment_reversal_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_payment_reversals_no", doc! { "reversal_no": 1 }),
        named_index(
            "idx_payment_reversals_original",
            doc! { "original_supplier_payment_id": 1, "status": 1 },
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

    use super::{
        customer_refund_indexes, payment_reversal_indexes, purchase_return_order_indexes,
        receipt_reversal_indexes, sales_return_case_indexes, sales_return_line_indexes,
        supplier_refund_indexes,
    };

    fn name(index: &IndexModel) -> Option<&str> {
        index.options.as_ref().and_then(|options| options.name.as_deref())
    }

    #[test]
    fn case_and_order_numbers_are_globally_unique() {
        assert!(sales_return_case_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_sales_return_cases_no")
                && index.options.as_ref().and_then(|o| o.unique) == Some(true)));
        assert!(purchase_return_order_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_purchase_return_orders_no")));
        assert!(sales_return_case_indexes().iter().any(|index| {
            name(index) == Some("idx_sales_return_cases_order_status")
                && index.keys == doc! { "sales_order_id": 1, "status": 1 }
        }));
    }

    #[test]
    fn refunds_and_reversals_cover_original_fact_lookups() {
        assert!(customer_refund_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_customer_refunds_no")));
        assert!(customer_refund_indexes().iter().any(|index| {
            name(index) == Some("idx_customer_refunds_original")
                && index.keys == doc! { "original_receipt_id": 1, "original_receivable_entry_id": 1 }
        }));
        assert!(supplier_refund_indexes()
            .iter()
            .any(|index| name(index) == Some("uk_supplier_refunds_no")));
        assert!(receipt_reversal_indexes()
            .iter()
            .any(|index| { index.keys == doc! { "original_customer_receipt_id": 1, "status": 1 } }));
        assert!(payment_reversal_indexes()
            .iter()
            .any(|index| { index.keys == doc! { "original_supplier_payment_id": 1, "status": 1 } }));
        assert!(sales_return_line_indexes()
            .iter()
            .any(|index| name(index) == Some("idx_sales_return_lines_order_line")));
    }
}
