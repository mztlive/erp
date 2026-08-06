//! 域 D29 `mall_order` 的索引声明：mall_order_fact(+cancel/completion)、mall_order、
//! mall_order_item、mall_payment_source、mall_item_funding_allocation、
//! mall_consumption_entry、mall_consumption_cost_assessment。
//!
//! 集合名常量取 `MallOrderExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。
//!
//! §6.17 逐条对照：
//! - `mall_order_fact`：「`business_fact_key` 业务事实层唯一」→
//!   `uk_mall_order_facts_business_key`（★去重唯一索引，不靠应用层查重）；
//!   「`inbox_message_id` 非空且唯一」→ `uk_mall_order_facts_inbox_message`；
//!   「`(mall_id, source_event_id)` 消息层唯一」→ `uk_mall_order_facts_source_event`；
//!   归集调度按状态扫描 → `idx_mall_order_facts_status`；按售后请求回溯 →
//!   `idx_mall_order_facts_after_sales_request`；
//! - 两张扩展表：「`mall_order_fact_id` 各自唯一」→
//!   `uk_mall_order_cancel_facts_fact`、`uk_mall_order_completion_facts_fact`；
//! - `mall_order`：「`(mall_id, external_order_no)` 唯一」→ `uk_mall_orders_identity`；
//!   「`payment_fact_id` 非空且唯一」→ `uk_mall_orders_payment_fact`；
//!   「`customer_id + paid_at`、`fulfillment_chain + paid_at` 查询索引」→
//!   `idx_mall_orders_customer_paid`、`idx_mall_orders_fulfillment_paid`；
//! - `mall_order_item`：「`(mall_order_id, external_item_id)` 唯一」→
//!   `uk_mall_order_items_identity`；「`sku_id + paid_at` 查询」的 `sku_id` 侧 →
//!   `idx_mall_order_items_sku`（`paid_at` 在订单头，跨集合由 Service 聚合）；
//! - `mall_payment_source`：「`(mall_order_id, source_no)` 唯一」→
//!   `uk_mall_payment_sources_no`；按卡实例追溯消费 →
//!   `idx_mall_payment_sources_card_instance`；
//! - `mall_item_funding_allocation`：「`(mall_order_item_id, mall_payment_source_id)`
//!   唯一」→ `uk_mall_item_funding_allocations_cell`；「商品明细和支付来源双向
//!   查询索引」→ `idx_mall_item_funding_allocations_source`；
//! - `mall_consumption_entry`：「同一业务事实、商品明细、支付来源和方向唯一」→
//!   `uk_mall_consumption_entries_fact_item_source`；「`origin_sales_order_id +
//!   occurred_at`、`attribution_status + occurred_at` 分析索引」→
//!   `idx_mall_consumption_entries_sales_order`、`idx_mall_consumption_entries_status`；
//! - `mall_consumption_cost_assessment`：「`(mall_consumption_entry_id,
//!   assessment_no)` 唯一」→ `uk_mall_consumption_cost_assessments_no`；
//!   「`supersedes_assessment_id` 非空时唯一」→
//!   `uk_mall_consumption_cost_assessments_supersedes`（稀疏唯一）。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::MallOrderExt;
use crate::Result;

/// `mall_order_fact` 集合名。
pub(crate) const MALL_ORDER_FACTS: &str = <mongodb::Database as MallOrderExt>::MALL_ORDER_FACTS;
/// `mall_order_cancel_fact` 集合名。
pub(crate) const MALL_ORDER_CANCEL_FACTS: &str = <mongodb::Database as MallOrderExt>::MALL_ORDER_CANCEL_FACTS;
/// `mall_order_completion_fact` 集合名。
pub(crate) const MALL_ORDER_COMPLETION_FACTS: &str =
    <mongodb::Database as MallOrderExt>::MALL_ORDER_COMPLETION_FACTS;
/// `mall_order` 集合名。
pub(crate) const MALL_ORDERS: &str = <mongodb::Database as MallOrderExt>::MALL_ORDERS;
/// `mall_order_item` 集合名。
pub(crate) const MALL_ORDER_ITEMS: &str = <mongodb::Database as MallOrderExt>::MALL_ORDER_ITEMS;
/// `mall_payment_source` 集合名。
pub(crate) const MALL_PAYMENT_SOURCES: &str = <mongodb::Database as MallOrderExt>::MALL_PAYMENT_SOURCES;
/// `mall_item_funding_allocation` 集合名。
pub(crate) const MALL_ITEM_FUNDING_ALLOCATIONS: &str =
    <mongodb::Database as MallOrderExt>::MALL_ITEM_FUNDING_ALLOCATIONS;
/// `mall_consumption_entry` 集合名。
pub(crate) const MALL_CONSUMPTION_ENTRIES: &str =
    <mongodb::Database as MallOrderExt>::MALL_CONSUMPTION_ENTRIES;
/// `mall_consumption_cost_assessment` 集合名。
pub(crate) const MALL_CONSUMPTION_COST_ASSESSMENTS: &str =
    <mongodb::Database as MallOrderExt>::MALL_CONSUMPTION_COST_ASSESSMENTS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.17「必需约束与索引」；唯一约束一律用唯一索引表达。
/// ★ `business_fact_key` 去重**只靠唯一索引**（P2 计划 §5），服务层不得做
/// 「先查后插」的重复性判断。成本评估链前驱「非空才唯一」用**稀疏唯一索引**
/// 表达：非稀疏唯一索引会把缺失字段视为 `null` 且只允许一个文档为空，而
/// `assessment_no = 1` 的评估恰好没有前驱字段，必须 `sparse` 才能表达
/// 「非空唯一」。回滚方式：删除稀疏唯一索引，改由序号唯一索引 + P3 链校验兜底。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, MALL_ORDER_FACTS, order_fact_indexes()).await?;
    create_indexes(db, MALL_ORDER_CANCEL_FACTS, cancel_fact_indexes()).await?;
    create_indexes(db, MALL_ORDER_COMPLETION_FACTS, completion_fact_indexes()).await?;
    create_indexes(db, MALL_ORDERS, mall_order_indexes()).await?;
    create_indexes(db, MALL_ORDER_ITEMS, mall_order_item_indexes()).await?;
    create_indexes(db, MALL_PAYMENT_SOURCES, payment_source_indexes()).await?;
    create_indexes(db, MALL_ITEM_FUNDING_ALLOCATIONS, funding_allocation_indexes()).await?;
    create_indexes(db, MALL_CONSUMPTION_ENTRIES, consumption_entry_indexes()).await?;
    create_indexes(db, MALL_CONSUMPTION_COST_ASSESSMENTS, cost_assessment_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `mall_order_fact` 的去重、消息去重与归集扫描索引。
fn order_fact_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_order_facts_business_key",
            doc! { "business_fact_key": 1 },
        ),
        unique_index(
            "uk_mall_order_facts_inbox_message",
            doc! { "inbox_message_id": 1 },
        ),
        unique_index(
            "uk_mall_order_facts_source_event",
            doc! { "mall_id": 1, "source_event_id": 1 },
        ),
        named_index("idx_mall_order_facts_status", doc! { "processing_status": 1 }),
        named_index(
            "idx_mall_order_facts_after_sales_request",
            doc! { "after_sales_request_id": 1 },
        ),
    ]
}

/// 返回 `mall_order_cancel_fact` 的事实一对一唯一索引。
fn cancel_fact_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_mall_order_cancel_facts_fact",
        doc! { "mall_order_fact_id": 1 },
    )]
}

/// 返回 `mall_order_completion_fact` 的事实一对一唯一索引。
fn completion_fact_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_mall_order_completion_facts_fact",
        doc! { "mall_order_fact_id": 1 },
    )]
}

/// 返回 `mall_order` 的身份、唯一支付事实与查询索引。
fn mall_order_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_orders_identity",
            doc! { "mall_id": 1, "external_order_no": 1 },
        ),
        unique_index("uk_mall_orders_payment_fact", doc! { "payment_fact_id": 1 }),
        named_index(
            "idx_mall_orders_customer_paid",
            doc! { "customer_id": 1, "paid_at": 1 },
        ),
        named_index(
            "idx_mall_orders_fulfillment_paid",
            doc! { "fulfillment_chain": 1, "paid_at": 1 },
        ),
    ]
}

/// 返回 `mall_order_item` 的明细唯一与 SKU 查询索引。
fn mall_order_item_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_order_items_identity",
            doc! { "mall_order_id": 1, "external_item_id": 1 },
        ),
        named_index("idx_mall_order_items_sku", doc! { "sku_id": 1 }),
    ]
}

/// 返回 `mall_payment_source` 的来源序号唯一与卡实例查询索引。
fn payment_source_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_payment_sources_no",
            doc! { "mall_order_id": 1, "source_no": 1 },
        ),
        named_index(
            "idx_mall_payment_sources_card_instance",
            doc! { "mall_card_instance_id": 1 },
        ),
    ]
}

/// 返回 `mall_item_funding_allocation` 的矩阵单元唯一与双向查询索引。
fn funding_allocation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_item_funding_allocations_cell",
            doc! { "mall_order_item_id": 1, "mall_payment_source_id": 1 },
        ),
        named_index(
            "idx_mall_item_funding_allocations_source",
            doc! { "mall_payment_source_id": 1 },
        ),
    ]
}

/// 返回 `mall_consumption_entry` 的消费唯一与分析索引。
fn consumption_entry_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_consumption_entries_fact_item_source",
            doc! {
                "mall_order_fact_id": 1,
                "mall_order_item_id": 1,
                "mall_payment_source_id": 1,
                "direction": 1,
            },
        ),
        named_index(
            "idx_mall_consumption_entries_sales_order",
            doc! { "origin_sales_order_id": 1, "occurred_at": 1 },
        ),
        named_index(
            "idx_mall_consumption_entries_status",
            doc! { "attribution_status": 1, "occurred_at": 1 },
        ),
    ]
}

/// 返回 `mall_consumption_cost_assessment` 的评估链约束索引。
fn cost_assessment_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_consumption_cost_assessments_no",
            doc! { "mall_consumption_entry_id": 1, "assessment_no": 1 },
        ),
        sparse_unique_index(
            "uk_mall_consumption_cost_assessments_supersedes",
            doc! { "supersedes_assessment_id": 1 },
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

/// 构建命名稀疏唯一索引（只对存在的字段施加唯一约束）。
fn sparse_unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder()
                .name(name.into())
                .unique(true)
                .sparse(true)
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        consumption_entry_indexes, cost_assessment_indexes, funding_allocation_indexes, mall_order_indexes,
        mall_order_item_indexes, order_fact_indexes, payment_source_indexes,
    };

    #[test]
    fn order_fact_dedup_indexes_are_all_unique() {
        let indexes = order_fact_indexes();
        for (name, keys) in [
            (
                "uk_mall_order_facts_business_key",
                doc! { "business_fact_key": 1 },
            ),
            (
                "uk_mall_order_facts_inbox_message",
                doc! { "inbox_message_id": 1 },
            ),
            (
                "uk_mall_order_facts_source_event",
                doc! { "mall_id": 1, "source_event_id": 1 },
            ),
        ] {
            let index = indexes
                .iter()
                .find(|index| {
                    index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name)
                })
                .unwrap_or_else(|| panic!("缺少索引 {name}"));
            assert_eq!(index.keys, keys);
            assert_eq!(index.options.as_ref().unwrap().unique, Some(true));
        }
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "processing_status": 1 } }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "after_sales_request_id": 1 } }));
    }

    #[test]
    fn mall_order_indexes_cover_identity_payment_fact_and_queries() {
        let indexes = mall_order_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_orders_identity")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys == doc! { "mall_id": 1, "external_order_no": 1 }
        }));
        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_orders_payment_fact")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys == doc! { "payment_fact_id": 1 }
        }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "customer_id": 1, "paid_at": 1 } }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "fulfillment_chain": 1, "paid_at": 1 } }));
    }

    #[test]
    fn consumption_entry_and_cost_assessment_cover_uniqueness_rules() {
        let entries = consumption_entry_indexes();
        assert!(entries.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_consumption_entries_fact_item_source")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys
                    == doc! {
                        "mall_order_fact_id": 1,
                        "mall_order_item_id": 1,
                        "mall_payment_source_id": 1,
                        "direction": 1,
                    }
        }));

        let assessments = cost_assessment_indexes();
        assert!(assessments.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_consumption_cost_assessments_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys == doc! { "mall_consumption_entry_id": 1, "assessment_no": 1 }
        }));
        let supersedes = assessments
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_consumption_cost_assessments_supersedes")
            })
            .unwrap();
        assert_eq!(supersedes.keys, doc! { "supersedes_assessment_id": 1 });
        assert_eq!(supersedes.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(supersedes.options.as_ref().unwrap().sparse, Some(true));
    }

    #[test]
    fn item_and_source_indexes_cover_identity_and_bidirectional_queries() {
        let items = mall_order_item_indexes();
        assert!(items.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_order_items_identity")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));

        let allocations = funding_allocation_indexes();
        assert!(allocations.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_item_funding_allocations_cell")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(allocations
            .iter()
            .any(|index| { index.keys == doc! { "mall_payment_source_id": 1 } }));

        let sources = payment_source_indexes();
        assert!(sources.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_payment_sources_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(sources
            .iter()
            .any(|index| { index.keys == doc! { "mall_card_instance_id": 1 } }));
    }
}
