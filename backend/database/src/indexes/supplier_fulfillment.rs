//! 域 D32 `supplier_fulfillment` 的索引声明：supplier_fulfillment_order、
//! supplier_fulfillment_item、supplier_order_action(+_line)、
//! supplier_order_status_history、supplier_refund_fact、supplier_refund_allocation。
//!
//! 集合名常量取 `SupplierFulfillmentExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SupplierFulfillmentExt;
use crate::Result;

/// `supplier_fulfillment_order` 集合名。
pub(crate) const SUPPLIER_FULFILLMENT_ORDERS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS;
/// `supplier_fulfillment_item` 集合名。
pub(crate) const SUPPLIER_FULFILLMENT_ITEMS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS;
/// `supplier_order_action` 集合名。
pub(crate) const SUPPLIER_ORDER_ACTIONS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTIONS;
/// `supplier_order_action_line` 集合名。
pub(crate) const SUPPLIER_ORDER_ACTION_LINES: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTION_LINES;
/// `supplier_order_status_history` 集合名。
pub(crate) const SUPPLIER_ORDER_STATUS_HISTORIES: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_STATUS_HISTORIES;
/// `supplier_refund_fact` 集合名。
pub(crate) const SUPPLIER_REFUND_FACTS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS;
/// `supplier_refund_allocation` 集合名。
pub(crate) const SUPPLIER_REFUND_ALLOCATIONS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.19「必需约束与索引」。身份类字段（订单号、商城明细归属、
/// 动作幂等键、回调/退款幂等键）使用**全局唯一索引**（与 accounts 的 code 处理一致）：
/// 软删除后仍保留身份，避免复用破坏恢复与幂等追溯语义；可空身份字段使用
/// **部分唯一索引**（非空时才参与唯一约束），保证 `NULL` 不阻塞多条未填写记录的写入。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(
        db,
        SUPPLIER_FULFILLMENT_ORDERS,
        supplier_fulfillment_order_indexes(),
    )
    .await?;
    create_indexes(
        db,
        SUPPLIER_FULFILLMENT_ITEMS,
        supplier_fulfillment_item_indexes(),
    )
    .await?;
    create_indexes(db, SUPPLIER_ORDER_ACTIONS, supplier_order_action_indexes()).await?;
    create_indexes(
        db,
        SUPPLIER_ORDER_ACTION_LINES,
        supplier_order_action_line_indexes(),
    )
    .await?;
    create_indexes(
        db,
        SUPPLIER_ORDER_STATUS_HISTORIES,
        supplier_order_status_history_indexes(),
    )
    .await?;
    create_indexes(db, SUPPLIER_REFUND_FACTS, supplier_refund_fact_indexes()).await?;
    create_indexes(
        db,
        SUPPLIER_REFUND_ALLOCATIONS,
        supplier_refund_allocation_indexes(),
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

/// 返回 `supplier_fulfillment_order` 的身份约束和查询索引（§6.19）。
fn supplier_fulfillment_order_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_fulfillment_orders_order_no",
            doc! { "fulfillment_order_no": 1 },
        ),
        unique_index(
            "uk_supplier_fulfillment_orders_mall_supplier_split",
            doc! { "mall_order_id": 1, "supplier_id": 1, "split_no": 1 },
        ),
        // §6.19「非空 (connection_id, external_order_no) 唯一」：部分唯一索引，
        // external_order_no 为空（下单成功回传前）时多条记录不受约束；
        // 回滚方式：删除本索引后以唯一约束下放到应用层校验。
        partial_unique_index(
            "uk_supplier_fulfillment_orders_connection_external",
            doc! { "connection_id": 1, "external_order_no": 1 },
            doc! { "external_order_no": { "$type": "string" } },
        ),
        named_index(
            "idx_supplier_fulfillment_orders_supplier_status_created",
            doc! { "supplier_id": 1, "fulfillment_status": 1, "created_at": -1 },
        ),
        named_index(
            "idx_supplier_fulfillment_orders_external_order_no",
            doc! { "external_order_no": 1 },
        ),
        // §6.19 mall_order_fact 联动：按来源商城订单聚合子订单的查询索引。
        named_index(
            "idx_supplier_fulfillment_orders_mall_order",
            doc! { "mall_order_id": 1 },
        ),
        // FUL-R06 结算来源范围：按供应商枚举期间内完成订单（completed_at 区间）
        // 的有界查询索引；回滚方式：删除本索引后查询退化为全表扫描。
        named_index(
            "idx_supplier_fulfillment_orders_supplier_completed",
            doc! { "supplier_id": 1, "completed_at": 1 },
        ),
    ]
}

/// 返回 `supplier_fulfillment_item` 的身份约束和明细查询索引（§6.19）。
fn supplier_fulfillment_item_indexes() -> Vec<IndexModel> {
    vec![
        // §6.19「一条商城商品明细只属于一个供应商子订单」：全局唯一，不拆量给多个供应商。
        unique_index(
            "uk_supplier_fulfillment_items_mall_order_item",
            doc! { "mall_order_item_id": 1 },
        ),
        named_index(
            "idx_supplier_fulfillment_items_order",
            doc! { "supplier_fulfillment_order_id": 1 },
        ),
    ]
}

/// 返回 `supplier_order_action` 的身份约束与查询索引（§6.19）。
///
/// `idx_supplier_order_actions_request` 支撑 FUL-R03 `scope_submitted_totals`
/// 按 `{after_sales_request_id, deleted_at}` 枚举动作头的有界查询；前向路径为
/// 本函数随 `ensure_indexes` 创建，回滚方式为删除本索引后查询退化为全表扫描
/// （合同 §7.3.5）。
fn supplier_order_action_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_order_actions_idempotency_key",
            doc! { "idempotency_key": 1 },
        ),
        named_index(
            "idx_supplier_order_actions_order",
            doc! { "supplier_fulfillment_order_id": 1 },
        ),
        named_index(
            "idx_supplier_order_actions_request",
            doc! { "after_sales_request_id": 1, "deleted_at": 1 },
        ),
    ]
}

/// 返回 `supplier_order_action_line` 的身份约束索引（§6.19）。
fn supplier_order_action_line_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_order_action_lines_action_line_no",
            doc! { "supplier_order_action_id": 1, "line_no": 1 },
        ),
        unique_index(
            "uk_supplier_order_action_lines_action_request_line",
            doc! { "supplier_order_action_id": 1, "after_sales_request_line_id": 1 },
        ),
    ]
}

/// 返回 `supplier_order_status_history` 的回调幂等唯一索引（§6.19）。
fn supplier_order_status_history_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_order_status_histories_connection_event",
        doc! { "connection_id": 1, "external_event_id": 1 },
    )]
}

/// 返回 `supplier_refund_fact` 的身份约束和订单查询索引（§6.19）。
fn supplier_refund_fact_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_refund_facts_connection_refund",
            doc! {
                "connection_id": 1,
                "external_refund_no": 1,
                "external_refund_version": 1,
            },
        ),
        // §6.19「inbox_message_id 非空且唯一」：必填字段，全局唯一索引。
        unique_index(
            "uk_supplier_refund_facts_inbox_message",
            doc! { "inbox_message_id": 1 },
        ),
        named_index(
            "idx_supplier_refund_facts_order",
            doc! { "supplier_fulfillment_order_id": 1 },
        ),
        // FUL-R06 结算来源范围：按供应商枚举期间内退款事实（refunded_at 区间）
        // 的有界查询索引；回滚方式：删除本索引后查询退化为全表扫描。
        named_index(
            "idx_supplier_refund_facts_supplier_refunded",
            doc! { "supplier_id": 1, "refunded_at": 1 },
        ),
    ]
}

/// 返回 `supplier_refund_allocation` 的身份约束索引（§6.19）。
fn supplier_refund_allocation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_refund_allocations_fact_no",
            doc! { "supplier_refund_fact_id": 1, "allocation_no": 1 },
        ),
        // §6.19「REVERSE 与原 APPLY 一对一」：部分唯一索引，仅 REVERSE 行
        // 的 reverses_allocation_id 参与唯一约束（APPLY 行为空不受约束）；
        // 回滚方式：删除本索引后以 P3 应用层校验替代。
        partial_unique_index(
            "uk_supplier_refund_allocations_reverse_source",
            doc! { "reverses_allocation_id": 1 },
            doc! { "allocation_action": { "$eq": "REVERSE" } },
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
        supplier_fulfillment_item_indexes, supplier_fulfillment_order_indexes, supplier_order_action_indexes,
        supplier_order_action_line_indexes, supplier_order_status_history_indexes,
        supplier_refund_allocation_indexes, supplier_refund_fact_indexes,
    };

    #[test]
    fn fulfillment_order_identity_and_query_indexes_match_section_6_19() {
        let indexes = supplier_fulfillment_order_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_fulfillment_orders_order_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_fulfillment_orders_mall_supplier_split")
                && index.keys == doc! { "mall_order_id": 1, "supplier_id": 1, "split_no": 1 }
        }));
        let external = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_fulfillment_orders_connection_external")
            })
            .unwrap();
        assert_eq!(external.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            external.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "external_order_no": { "$type": "string" } })
        );
        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "supplier_id": 1, "fulfillment_status": 1, "created_at": -1 }
        }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "external_order_no": 1 }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "mall_order_id": 1 }));
    }

    #[test]
    fn fulfillment_item_ownership_is_globally_unique() {
        let indexes = supplier_fulfillment_item_indexes();

        let ownership = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_fulfillment_items_mall_order_item")
            })
            .unwrap();
        assert_eq!(ownership.keys, doc! { "mall_order_item_id": 1 });
        assert_eq!(ownership.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn action_request_index_covers_after_sales_scope_query() {
        let actions = supplier_order_action_indexes();

        let request = actions
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("idx_supplier_order_actions_request")
            })
            .unwrap();
        assert_eq!(
            request.keys,
            doc! { "after_sales_request_id": 1, "deleted_at": 1 },
            "FUL-R03 按售后申请枚举动作头必须命中该索引"
        );
        assert_ne!(
            request.options.as_ref().and_then(|options| options.unique),
            Some(true),
            "查询索引不得携带唯一约束"
        );
    }

    #[test]
    fn action_and_line_identity_indexes_are_unique() {
        assert!(supplier_order_action_indexes().iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_order_actions_idempotency_key")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));

        let lines = supplier_order_action_line_indexes();
        assert!(lines.iter().any(|index| {
            index.keys == doc! { "supplier_order_action_id": 1, "line_no": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(lines.iter().any(|index| {
            index.keys == doc! { "supplier_order_action_id": 1, "after_sales_request_line_id": 1 }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }

    #[test]
    fn status_history_and_refund_fact_idempotency_indexes_are_unique() {
        assert!(supplier_order_status_history_indexes().iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_order_status_histories_connection_event")
                && index.keys == doc! { "connection_id": 1, "external_event_id": 1 }
        }));

        let facts = supplier_refund_fact_indexes();
        assert!(facts.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_refund_facts_connection_refund")
                && index.keys
                    == doc! {
                        "connection_id": 1,
                        "external_refund_no": 1,
                        "external_refund_version": 1,
                    }
        }));
        assert!(facts.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_refund_facts_inbox_message")
        }));
    }

    #[test]
    fn refund_allocation_reverse_is_partially_unique() {
        let allocations = supplier_refund_allocation_indexes();

        assert!(allocations.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_supplier_refund_allocations_fact_no")
        }));
        let reverse = allocations
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_refund_allocations_reverse_source")
            })
            .unwrap();
        assert_eq!(reverse.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            reverse.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "allocation_action": { "$eq": "REVERSE" } })
        );
    }

    #[test]
    fn settlement_source_scope_range_indexes_are_present() {
        let orders = supplier_fulfillment_order_indexes();
        let completed = orders
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("idx_supplier_fulfillment_orders_supplier_completed")
            })
            .expect("供应商完成时间范围索引缺失");
        assert_eq!(completed.keys, doc! { "supplier_id": 1, "completed_at": 1 });
        assert_ne!(completed.options.as_ref().unwrap().unique, Some(true));

        let facts = supplier_refund_fact_indexes();
        let refunded = facts
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("idx_supplier_refund_facts_supplier_refunded")
            })
            .expect("供应商退款时间范围索引缺失");
        assert_eq!(refunded.keys, doc! { "supplier_id": 1, "refunded_at": 1 });
        assert_ne!(refunded.options.as_ref().unwrap().unique, Some(true));
    }
}
