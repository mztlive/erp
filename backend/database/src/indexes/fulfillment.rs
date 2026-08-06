//! 域 D16 `fulfillment` 的索引声明：purchase_receipt(+_line)、delivery(+_line)、
//! electronic_delivery、service_fulfillment、customer_acceptance(+_line)、
//! acceptance_fulfillment_allocation。
//!
//! 集合名常量取 `FulfillmentExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。
//!
//! 逐条落地数据模型 §6.7「必需约束与索引」：
//! - 单据身份类字段（receipt_no/delivery_no/fulfillment_no/acceptance_no）使用
//!   **全局唯一索引**（与 accounts 的 code 处理一致）：草稿单据可逻辑删除
//!   （§4.5.2），软删除后仍保留身份，避免单号复用破坏追溯与恢复语义；
//! - 行级约束 `(header_id, line_no)` 全局唯一：行不设业务软删除；
//! - 其余为列表/详情查询索引。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::FulfillmentExt;
use crate::Result;

/// `purchase_receipt` 集合名。
pub(crate) const PURCHASE_RECEIPTS: &str = <mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS;
/// `purchase_receipt_line` 集合名。
pub(crate) const PURCHASE_RECEIPT_LINES: &str = <mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPT_LINES;
/// `delivery` 集合名。
pub(crate) const DELIVERIES: &str = <mongodb::Database as FulfillmentExt>::DELIVERIES;
/// `delivery_line` 集合名。
pub(crate) const DELIVERY_LINES: &str = <mongodb::Database as FulfillmentExt>::DELIVERY_LINES;
/// `electronic_delivery` 集合名。
pub(crate) const ELECTRONIC_DELIVERIES: &str = <mongodb::Database as FulfillmentExt>::ELECTRONIC_DELIVERIES;
/// `service_fulfillment` 集合名。
pub(crate) const SERVICE_FULFILLMENTS: &str = <mongodb::Database as FulfillmentExt>::SERVICE_FULFILLMENTS;
/// `customer_acceptance` 集合名。
pub(crate) const CUSTOMER_ACCEPTANCES: &str = <mongodb::Database as FulfillmentExt>::CUSTOMER_ACCEPTANCES;
/// `customer_acceptance_line` 集合名。
pub(crate) const CUSTOMER_ACCEPTANCE_LINES: &str =
    <mongodb::Database as FulfillmentExt>::CUSTOMER_ACCEPTANCE_LINES;
/// `acceptance_fulfillment_allocation` 集合名。
pub(crate) const ACCEPTANCE_FULFILLMENT_ALLOCATIONS: &str =
    <mongodb::Database as FulfillmentExt>::ACCEPTANCE_FULFILLMENT_ALLOCATIONS;

/// 创建本域集合的幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, PURCHASE_RECEIPTS, purchase_receipt_indexes()).await?;
    create_indexes(db, PURCHASE_RECEIPT_LINES, purchase_receipt_line_indexes()).await?;
    create_indexes(db, DELIVERIES, delivery_indexes()).await?;
    create_indexes(db, DELIVERY_LINES, delivery_line_indexes()).await?;
    create_indexes(db, ELECTRONIC_DELIVERIES, electronic_delivery_indexes()).await?;
    create_indexes(db, SERVICE_FULFILLMENTS, service_fulfillment_indexes()).await?;
    create_indexes(db, CUSTOMER_ACCEPTANCES, customer_acceptance_indexes()).await?;
    create_indexes(db, CUSTOMER_ACCEPTANCE_LINES, customer_acceptance_line_indexes()).await?;
    create_indexes(db, ACCEPTANCE_FULFILLMENT_ALLOCATIONS, allocation_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `purchase_receipt` 的身份约束和采购维度查询索引（§6.7）。
fn purchase_receipt_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_purchase_receipts_receipt_no", doc! { "receipt_no": 1 }),
        named_index(
            "idx_purchase_receipts_po_status_posted",
            doc! { "purchase_order_id": 1, "status": 1, "posted_at": 1 },
        ),
    ]
}

/// 返回 `purchase_receipt_line` 的行级唯一约束（§6.7）。
fn purchase_receipt_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_purchase_receipt_lines_header_line",
        doc! { "purchase_receipt_id": 1, "line_no": 1 },
    )]
}

/// 返回 `delivery` 的身份约束和销售维度/物流查询索引（§6.7）。
fn delivery_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_deliveries_delivery_no", doc! { "delivery_no": 1 }),
        named_index(
            "idx_deliveries_sales_order_status",
            doc! { "sales_order_id": 1, "status": 1 },
        ),
        named_index("idx_deliveries_tracking_no", doc! { "tracking_no": 1 }),
    ]
}

/// 返回 `delivery_line` 的行级唯一约束（§6.7）。
fn delivery_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_delivery_lines_header_line",
        doc! { "delivery_id": 1, "line_no": 1 },
    )]
}

/// 返回 `electronic_delivery` 的身份约束和明细履约查询索引（§6.7）。
fn electronic_delivery_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_electronic_deliveries_fulfillment_no",
            doc! { "fulfillment_no": 1 },
        ),
        named_index(
            "idx_electronic_deliveries_line_occurred",
            doc! { "sales_order_line_id": 1, "occurred_at": 1 },
        ),
    ]
}

/// 返回 `service_fulfillment` 的身份约束和明细履约查询索引（§6.7）。
fn service_fulfillment_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_service_fulfillments_fulfillment_no",
            doc! { "fulfillment_no": 1 },
        ),
        named_index(
            "idx_service_fulfillments_line_occurred",
            doc! { "sales_order_line_id": 1, "occurred_at": 1 },
        ),
    ]
}

/// 返回 `customer_acceptance` 的身份约束和销售维度查询索引（§6.7）。
fn customer_acceptance_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_customer_acceptances_acceptance_no",
            doc! { "acceptance_no": 1 },
        ),
        named_index(
            "idx_customer_acceptances_sales_order_accepted",
            doc! { "sales_order_id": 1, "accepted_at": 1 },
        ),
    ]
}

/// 返回 `customer_acceptance_line` 的行级唯一约束（§6.7）。
fn customer_acceptance_line_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_customer_acceptance_lines_header_line",
        doc! { "customer_acceptance_id": 1, "line_no": 1 },
    )]
}

/// 返回 `acceptance_fulfillment_allocation` 的净验收分配查询索引（§6.7）。
///
/// 关单只使用有效履约事实和净 `APPLY - REVERSE` 验收分配（§6.7）：按验收行
/// 与按履约事实（类型 + 事实行）双向取数。
fn allocation_indexes() -> Vec<IndexModel> {
    vec![
        named_index(
            "idx_acceptance_fulfillment_allocations_acceptance_line",
            doc! { "customer_acceptance_line_id": 1 },
        ),
        named_index(
            "idx_acceptance_fulfillment_allocations_fulfillment_fact",
            doc! { "fulfillment_fact_type": 1, "fulfillment_line_id": 1 },
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

    use super::{
        allocation_indexes, customer_acceptance_indexes, customer_acceptance_line_indexes, delivery_indexes,
        delivery_line_indexes, electronic_delivery_indexes, purchase_receipt_indexes,
        purchase_receipt_line_indexes, service_fulfillment_indexes,
    };

    fn unique_keys(indexes: &[mongodb::IndexModel], name: &str) -> mongodb::bson::Document {
        indexes
            .iter()
            .find(|index| index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name))
            .map(|index| index.keys.clone())
            .unwrap_or_else(|| panic!("索引 {name} 缺失"))
    }

    #[test]
    fn purchase_receipt_indexes_cover_identity_and_po_dimension() {
        let indexes = purchase_receipt_indexes();
        assert_eq!(
            unique_keys(&indexes, "uk_purchase_receipts_receipt_no"),
            doc! { "receipt_no": 1 }
        );
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "purchase_order_id": 1, "status": 1, "posted_at": 1 } }));
        assert!(purchase_receipt_indexes().iter().all(|index| index
            .options
            .as_ref()
            .unwrap()
            .name
            .is_some()));
    }

    #[test]
    fn header_line_identity_indexes_are_globally_unique() {
        let pairs = [
            (
                purchase_receipt_line_indexes(),
                "uk_purchase_receipt_lines_header_line",
                doc! { "purchase_receipt_id": 1, "line_no": 1 },
            ),
            (
                delivery_line_indexes(),
                "uk_delivery_lines_header_line",
                doc! { "delivery_id": 1, "line_no": 1 },
            ),
            (
                customer_acceptance_line_indexes(),
                "uk_customer_acceptance_lines_header_line",
                doc! { "customer_acceptance_id": 1, "line_no": 1 },
            ),
        ];
        for (indexes, name, expected) in pairs {
            let index = indexes
                .iter()
                .find(|index| {
                    index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name)
                })
                .unwrap();
            assert_eq!(index.keys, expected);
            assert_eq!(index.options.as_ref().unwrap().unique, Some(true));
        }
    }

    #[test]
    fn delivery_indexes_cover_sales_order_and_tracking() {
        let indexes = delivery_indexes();
        assert_eq!(
            unique_keys(&indexes, "uk_deliveries_delivery_no"),
            doc! { "delivery_no": 1 }
        );
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "sales_order_id": 1, "status": 1 } }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "tracking_no": 1 }));
    }

    #[test]
    fn fact_indexes_cover_fulfillment_identity_and_line_timeline() {
        let pairs = [
            (
                electronic_delivery_indexes(),
                "uk_electronic_deliveries_fulfillment_no",
            ),
            (
                service_fulfillment_indexes(),
                "uk_service_fulfillments_fulfillment_no",
            ),
        ];
        for (indexes, name) in pairs {
            let identity = indexes
                .iter()
                .find(|index| {
                    index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name)
                })
                .unwrap();
            assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));
            assert!(indexes
                .iter()
                .any(|index| index.keys == doc! { "sales_order_line_id": 1, "occurred_at": 1 }));
        }
    }

    #[test]
    fn acceptance_indexes_cover_identity_sales_and_allocation_lookups() {
        let indexes = customer_acceptance_indexes();
        assert_eq!(
            unique_keys(&indexes, "uk_customer_acceptances_acceptance_no"),
            doc! { "acceptance_no": 1 }
        );
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "sales_order_id": 1, "accepted_at": 1 }));

        let allocation = allocation_indexes();
        assert!(allocation
            .iter()
            .any(|index| { index.keys == doc! { "customer_acceptance_line_id": 1 } }));
        assert!(allocation
            .iter()
            .any(|index| { index.keys == doc! { "fulfillment_fact_type": 1, "fulfillment_line_id": 1 } }));
    }
}
