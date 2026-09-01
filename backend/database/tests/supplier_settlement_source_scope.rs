//! FUL-R06 结算来源范围最小事实查询（Repository 下沉）的真实 MongoDB 验收。
//!
//! 覆盖：大量期外历史订单/退款不被读取；期间内完成与退款事实（含 `Asia/Shanghai`
//! 边界最后一秒）被取回；软删除与其他供应商事实被排除；按明细主键单独引用的
//! 订单被补取；分配与其事实头关联完整；新范围索引被查询计划使用。

use std::str::FromStr;

use database::{ensure_indexes, NoTransaction, SupplierFulfillmentExt, SupplierSettlementExt};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    CostAllocationId, CostEntryId, InboxMessageId, MallOrderId, MallOrderItemId, PayableEntryId,
    SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
    SupplierOfferingRevisionId, SupplierRefundAllocationId, SupplierRefundFactId,
};
use entities::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
use entities::supplier_fulfillment::{
    AllocationAction, FulfillmentStatus, SupplierFulfillmentItem, SupplierFulfillmentItemData,
    SupplierFulfillmentOrder, SupplierFulfillmentOrderData, SupplierRefundAllocation,
    SupplierRefundAllocationData, SupplierRefundFact, SupplierRefundFactData,
};
use mongodb::bson::{doc, Bson};
use test_support::{require_mongo, TestDb};

/// 解析 RFC3339 时刻为秒级时间戳（夹具统一用 `+08:00` 业务时区）。
fn instant(rfc3339: &str) -> Instant {
    Instant::from_unix_secs(chrono::DateTime::parse_from_rfc3339(rfc3339).unwrap().timestamp())
}

/// 已完成履约订单夹具（可选软删除；完成时间可精确指定）。
fn completed_order(
    id: &str,
    supplier_id: &SupplierAccountId,
    completed_at: Instant,
    deleted: bool,
) -> SupplierFulfillmentOrder {
    let mut order = SupplierFulfillmentOrder::new(
        SupplierFulfillmentOrderId::new(id),
        SupplierFulfillmentOrderData::submitting(
            format!("SO-{id}"),
            MallOrderId::new(format!("mall-{id}")),
            supplier_id.clone(),
            SupplierApiConnectionId::new("connection-1"),
            1,
            Instant::from_unix_secs(1_700_000_000),
            "encrypted-address",
            "fingerprint-address",
        ),
    )
    .expect("订单构造失败");
    order
        .advance_fulfillment(FulfillmentStatus::Accepted)
        .expect("迁移失败");
    order
        .advance_fulfillment(FulfillmentStatus::Fulfilling)
        .expect("迁移失败");
    order
        .advance_fulfillment(FulfillmentStatus::Completed)
        .expect("迁移失败");
    order.completed_at = Some(completed_at);
    if deleted {
        order.base.deleted_at = 1_700_000_001;
    }
    order
}

/// 履约明细夹具（成本快照按单价 × 数量对分舍入）。
fn fulfillment_item(id: &str, order_id: &SupplierFulfillmentOrderId) -> SupplierFulfillmentItem {
    let unit_price = UnitPrice::from_str("113.0000").unwrap();
    let quantity = Quantity::from_str("1.000000").unwrap();
    let (gross, _, _) = line_amounts(unit_price, quantity, Rate::from_str("0.130000").unwrap());
    SupplierFulfillmentItem::new(
        SupplierFulfillmentItemId::new(id),
        SupplierFulfillmentItemData {
            supplier_fulfillment_order_id: order_id.clone(),
            mall_order_item_id: MallOrderItemId::new(format!("mall-item-{id}")),
            supplier_offering_revision_id: SupplierOfferingRevisionId::new("offering-rev-1"),
            supplier_sku_code_snapshot: format!("SKU-{id}"),
            supplier_product_code_snapshot: None,
            quantity,
            unit_cost_snapshot_gross: unit_price,
            cost_snapshot_total_gross: gross,
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
        },
    )
    .expect("明细构造失败")
}

/// 退款事实夹具。
fn refund_fact(
    id: &str,
    supplier_id: &SupplierAccountId,
    order_id: &SupplierFulfillmentOrderId,
    refunded_at: Instant,
) -> SupplierRefundFact {
    SupplierRefundFact::new(
        SupplierRefundFactId::new(id),
        SupplierRefundFactData {
            supplier_id: supplier_id.clone(),
            connection_id: SupplierApiConnectionId::new("connection-1"),
            supplier_fulfillment_order_id: order_id.clone(),
            external_refund_no: format!("RF-{id}"),
            external_refund_version: "1".to_string(),
            refund_amount: Amount::from_str("10.00").unwrap(),
            refunded_at,
            source_event_id: format!("event-{id}"),
            inbox_message_id: InboxMessageId::new(format!("inbox-{id}")),
        },
    )
    .expect("退款事实构造失败")
}

/// 退款分配夹具（APPLY，金额恒等守恒）。
fn refund_allocation(
    id: &str,
    fact_id: &SupplierRefundFactId,
    item_id: &SupplierFulfillmentItemId,
) -> SupplierRefundAllocation {
    SupplierRefundAllocation::new(
        SupplierRefundAllocationId::new(id),
        SupplierRefundAllocationData {
            supplier_refund_fact_id: fact_id.clone(),
            allocation_no: 1,
            supplier_fulfillment_item_id: item_id.clone(),
            original_cost_entry_id: CostEntryId::new(format!("cost-{id}")),
            original_cost_allocation_id: CostAllocationId::new(format!("cost-alloc-{id}")),
            original_payable_entry_id: PayableEntryId::new(format!("payable-{id}")),
            original_payment_allocation_id: None,
            refund_quantity: Quantity::from_str("1.000000").unwrap(),
            gross_amount: Amount::from_str("10.00").unwrap(),
            net_amount: Amount::from_str("8.70").unwrap(),
            tax_amount: Amount::from_str("1.30").unwrap(),
            payable_reduction_amount: Amount::from_str("10.00").unwrap(),
            cash_refund_amount: Amount::from_str("0.00").unwrap(),
            cash_supplier_refund_id: None,
            allocation_action: AllocationAction::Apply,
            reverses_allocation_id: None,
        },
    )
    .expect("退款分配构造失败")
}

/// 递归检查 explain 文档是否命中指定索引名。
fn explain_uses_index(value: &Bson, expected: &str) -> bool {
    match value {
        Bson::Document(document) => {
            document.get("indexName").and_then(Bson::as_str) == Some(expected)
                || document
                    .iter()
                    .any(|(_, child)| explain_uses_index(child, expected))
        }
        Bson::Array(values) => values.iter().any(|child| explain_uses_index(child, expected)),
        _ => false,
    }
}

/// 期间外历史订单/退款不被读取；期间内完成与退款事实（含边界）被取回；
/// 软删除与其他供应商事实被排除；按明细主键单独引用的订单被补取。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn settlement_source_scope_is_bounded_and_associates_period_facts() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_settlement_source_scope")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let supplier = SupplierAccountId::new("supplier-1");
        let other_supplier = SupplierAccountId::new("supplier-2");

        // 订单：请求引用（期外完成）、期间内完成、边界最后一秒、期外完成、
        // 已删除、其他供应商、仅按明细引用（期外完成）。
        let orders = vec![
            completed_order(
                "order-requested",
                &supplier,
                instant("2026-06-01T00:00:00+08:00"),
                false,
            ),
            completed_order(
                "order-in-period",
                &supplier,
                instant("2026-07-15T12:00:00+08:00"),
                false,
            ),
            completed_order(
                "order-boundary",
                &supplier,
                instant("2026-07-31T23:59:59+08:00"),
                false,
            ),
            completed_order(
                "order-out",
                &supplier,
                instant("2026-08-01T00:00:00+08:00"),
                false,
            ),
            completed_order(
                "order-deleted",
                &supplier,
                instant("2026-07-10T00:00:00+08:00"),
                true,
            ),
            completed_order(
                "order-other",
                &other_supplier,
                instant("2026-07-15T12:00:00+08:00"),
                false,
            ),
            completed_order(
                "order-orphan",
                &supplier,
                instant("2026-06-01T00:00:00+08:00"),
                false,
            ),
        ];
        db.collection::<SupplierFulfillmentOrder>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
        )
        .insert_many(orders)
        .await
        .expect("订单插入失败");

        let items = vec![
            fulfillment_item(
                "item-requested",
                &SupplierFulfillmentOrderId::new("order-requested"),
            ),
            fulfillment_item(
                "item-in-period",
                &SupplierFulfillmentOrderId::new("order-in-period"),
            ),
            fulfillment_item(
                "item-boundary",
                &SupplierFulfillmentOrderId::new("order-boundary"),
            ),
            fulfillment_item("item-out", &SupplierFulfillmentOrderId::new("order-out")),
            fulfillment_item("item-other", &SupplierFulfillmentOrderId::new("order-other")),
            fulfillment_item("item-orphan", &SupplierFulfillmentOrderId::new("order-orphan")),
        ];
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_many(items)
        .await
        .expect("明细插入失败");

        let facts = vec![
            refund_fact(
                "fact-in-period",
                &supplier,
                &SupplierFulfillmentOrderId::new("order-in-period"),
                instant("2026-07-20T10:00:00+08:00"),
            ),
            refund_fact(
                "fact-out",
                &supplier,
                &SupplierFulfillmentOrderId::new("order-requested"),
                instant("2026-08-01T00:00:00+08:00"),
            ),
            refund_fact(
                "fact-other",
                &other_supplier,
                &SupplierFulfillmentOrderId::new("order-other"),
                instant("2026-07-20T10:00:00+08:00"),
            ),
        ];
        db.collection::<SupplierRefundFact>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
        )
        .insert_many(facts)
        .await
        .expect("退款事实插入失败");

        let allocations = vec![
            refund_allocation(
                "alloc-in",
                &SupplierRefundFactId::new("fact-in-period"),
                &SupplierFulfillmentItemId::new("item-in-period"),
            ),
            refund_allocation(
                "alloc-out",
                &SupplierRefundFactId::new("fact-out"),
                &SupplierFulfillmentItemId::new("item-requested"),
            ),
            refund_allocation(
                "alloc-other",
                &SupplierRefundFactId::new("fact-other"),
                &SupplierFulfillmentItemId::new("item-other"),
            ),
        ];
        db.collection::<SupplierRefundAllocation>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS,
        )
        .insert_many(allocations)
        .await
        .expect("退款分配插入失败");

        let scope = db
            .supplier_settlement()
            .settlement_source_scope(
                &supplier,
                BusinessDate::from_str("2026-07-01").unwrap(),
                BusinessDate::from_str("2026-07-31").unwrap(),
                &[SupplierFulfillmentOrderId::new("order-requested")],
                &[
                    SupplierFulfillmentItemId::new("item-requested"),
                    SupplierFulfillmentItemId::new("item-orphan"),
                ],
                &mut NoTransaction,
            )
            .await
            .expect("范围查询失败");

        let order_ids = scope
            .orders
            .iter()
            .map(|order| order.base.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            order_ids,
            vec![
                "order-boundary",
                "order-in-period",
                "order-orphan",
                "order-requested",
            ],
            "只取回请求引用、期间内完成（含边界最后一秒）与按明细引用的订单，且按主键稳定排序；\
             期外、已删除、其他供应商订单一律不取"
        );
        let item_ids = scope
            .items
            .iter()
            .map(|item| item.base.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            item_ids,
            vec!["item-boundary", "item-in-period", "item-orphan", "item-requested"],
            "只取回范围订单的明细与请求显式引用的明细"
        );
        let fact_ids = scope
            .refund_facts
            .iter()
            .map(|fact| fact.base.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(fact_ids, vec!["fact-in-period"], "只取回期间内退款事实");
        let allocation_ids = scope
            .refund_allocations
            .iter()
            .map(|allocation| allocation.base.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(allocation_ids, vec!["alloc-in"], "只取回期间内退款事实的分配");

        // 分配与其事实头关联完整（Repository 负责批量关联）。
        assert!(scope.refund_allocations.iter().all(|allocation| {
            scope
                .refund_facts
                .iter()
                .any(|fact| fact.base.id == allocation.supplier_refund_fact_id.to_string())
        }));
        // 期间内完成订单的明细必须全部在场，供 Service 的完整性检查 fail-closed。
        assert!(scope.items.iter().all(|item| scope
            .orders
            .iter()
            .any(|order| order.base.id == item.supplier_fulfillment_order_id.to_string())));

        // 代表性 explain：期间内退款事实查询命中新范围索引。
        let start = instant("2026-07-01T00:00:00+08:00").unix_secs();
        let end = instant("2026-08-01T00:00:00+08:00").unix_secs();
        let explain = db
            .run_command(doc! {
                "explain": {
                    "find": <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
                    "filter": {
                        "supplier_id": supplier.to_string(),
                        "refunded_at": { "$gte": start, "$lt": end },
                        "deleted_at": 0,
                    },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("explain 失败");
        assert!(
            explain_uses_index(
                &Bson::Document(explain.clone()),
                "idx_supplier_refund_facts_supplier_refunded"
            ),
            "期间内退款事实查询必须命中 idx_supplier_refund_facts_supplier_refunded"
        );
        eprintln!("EXPLAIN refund_facts winning plan: {explain:?}");

        // 代表性 explain：期间内完成订单范围分支（supplier_id + completed_at
        // $gte/$lt + deleted_at，即 order_scope_filter 的第二分支）命中新索引。
        let explain = db
            .run_command(doc! {
                "explain": {
                    "find": <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
                    "filter": {
                        "supplier_id": supplier.to_string(),
                        "completed_at": { "$gte": start, "$lt": end },
                        "deleted_at": 0,
                    },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("orders explain 失败");
        assert!(
            explain_uses_index(
                &Bson::Document(explain.clone()),
                "idx_supplier_fulfillment_orders_supplier_completed"
            ),
            "期间内完成订单范围查询必须命中 idx_supplier_fulfillment_orders_supplier_completed"
        );
        eprintln!("EXPLAIN orders winning plan: {explain:?}");
    });
}
