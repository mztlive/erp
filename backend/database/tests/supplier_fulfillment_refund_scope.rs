//! FUL-R03/FUL-R05 售后动作范围与退款财务快照（Repository 下沉）的真实 MongoDB 验收。
//!
//! 覆盖：六态逐一及混合累计；软删动作头/行排除；跨订单/申请隔离；无历史为
//! 精确零；多明细成本与多退款事实累加；已删除事实不计入；金额精度与相等边界；
//! 事务内同一 session 可见未提交写入；两个并发退款不能共同突破净可退余额
//! （订单 CAS 失败关闭）；代表性 explain 命中新动作头查询索引。

use std::str::FromStr;

use database::{ensure_indexes, MallAfterSalesExt, NoTransaction, SupplierFulfillmentExt, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    InboxMessageId, MallAfterSalesRequestId, MallAfterSalesRequestLineId, MallOrderId, MallOrderItemId,
    SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
    SupplierOfferingRevisionId, SupplierOrderActionId, SupplierOrderActionLineId, SupplierRefundFactId,
};
use entities::mall_after_sales::{
    AfterSalesLineStatus, MallAfterSalesRequestLine, MallAfterSalesRequestLineData,
};
use entities::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
use entities::supplier_fulfillment::{
    SupplierFulfillmentItem, SupplierFulfillmentItemData, SupplierFulfillmentOrder,
    SupplierFulfillmentOrderData, SupplierOrderAction, SupplierOrderActionData, SupplierOrderActionLine,
    SupplierOrderActionLineData, SupplierOrderActionStatus, SupplierOrderActionType, SupplierRefundFact,
    SupplierRefundFactData,
};
use mongodb::bson::{doc, Bson};
use mongodb::Database;
use test_support::{require_mongo, TestDb};

/// 履约订单夹具（仅并发 CAS 测试需要订单实体）。
fn order(id: &str, supplier_id: &SupplierAccountId) -> SupplierFulfillmentOrder {
    SupplierFulfillmentOrder::new(
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
    .expect("订单构造失败")
}

/// 履约明细夹具（成本快照按单价 × 数量对分舍入；可选软删除）。
fn fulfillment_item(
    id: &str,
    order_id: &SupplierFulfillmentOrderId,
    unit_price: &str,
    quantity: &str,
    deleted: bool,
) -> SupplierFulfillmentItem {
    let unit_price = UnitPrice::from_str(unit_price).unwrap();
    let quantity = Quantity::from_str(quantity).unwrap();
    let (gross, _, _) = line_amounts(unit_price, quantity, Rate::from_str("0.130000").unwrap());
    let mut item = SupplierFulfillmentItem::new(
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
    .expect("明细构造失败");
    if deleted {
        item.base.deleted_at = 1_700_000_001;
    }
    item
}

/// 商城售后申请行夹具（`(after_sales_request_id, line_no)` 唯一；可选软删除）。
fn request_line(
    id: &str,
    request_id: &MallAfterSalesRequestId,
    item_id: &SupplierFulfillmentItemId,
    line_no: u32,
    requested_quantity: &str,
    requested_amount: &str,
    deleted: bool,
) -> MallAfterSalesRequestLine {
    let mut line = MallAfterSalesRequestLine::new(
        MallAfterSalesRequestLineId::new(id),
        MallAfterSalesRequestLineData {
            after_sales_request_id: request_id.clone(),
            line_no,
            mall_order_item_id: MallOrderItemId::new(format!("mall-line-{id}")),
            supplier_fulfillment_item_id: Some(item_id.clone()),
            requested_quantity: Quantity::from_str(requested_quantity).unwrap(),
            requested_amount: Amount::from_str(requested_amount).unwrap(),
            line_status: AfterSalesLineStatus::Pending,
        },
    )
    .expect("申请行构造失败");
    if deleted {
        line.base.deleted_at = 1_700_000_001;
    }
    line
}

/// 售后动作头夹具（取消/退款动作必填售后申请；可选软删除）。
///
/// 只读路径按 `after_sales_request_id` 聚合，与幂等键无关；同一申请下多个
/// 动作头为六态合成夹具，幂等键取动作 ID 保证唯一索引不冲突（写入侧幂等
/// 约束不在本读路径验收范围）。
fn action(
    id: &str,
    order_id: &SupplierFulfillmentOrderId,
    request_id: &MallAfterSalesRequestId,
    status: SupplierOrderActionStatus,
    deleted: bool,
) -> SupplierOrderAction {
    let mut action = SupplierOrderAction::new(
        SupplierOrderActionId::new(id),
        SupplierOrderActionData {
            supplier_fulfillment_order_id: order_id.clone(),
            action_type: SupplierOrderActionType::Refund,
            after_sales_request_id: Some(request_id.clone()),
            idempotency_key: format!("seed-{id}"),
            status,
            external_request_id: None,
            request_summary: None,
            response_summary: None,
            attempt_count: 0,
            next_attempt_at: None,
        },
    )
    .expect("动作头构造失败");
    if deleted {
        action.base.deleted_at = 1_700_000_001;
    }
    action
}

/// 售后动作行夹具（数量与金额必须大于零；可选软删除）。
fn action_line(
    id: &str,
    action_id: &SupplierOrderActionId,
    request_line_id: &MallAfterSalesRequestLineId,
    item_id: &SupplierFulfillmentItemId,
    quantity: &str,
    amount: &str,
    deleted: bool,
) -> SupplierOrderActionLine {
    let mut line = SupplierOrderActionLine::new(
        SupplierOrderActionLineId::new(id),
        SupplierOrderActionLineData {
            supplier_order_action_id: action_id.clone(),
            line_no: 1,
            after_sales_request_line_id: request_line_id.clone(),
            supplier_fulfillment_item_id: item_id.clone(),
            quantity: Quantity::from_str(quantity).unwrap(),
            amount: Amount::from_str(amount).unwrap(),
        },
    )
    .expect("动作行构造失败");
    if deleted {
        line.base.deleted_at = 1_700_000_001;
    }
    line
}

/// 退款事实夹具（可选软删除；软删除事实不得计入退款累计）。
fn refund_fact(
    id: &str,
    order_id: &SupplierFulfillmentOrderId,
    amount: &str,
    deleted: bool,
) -> SupplierRefundFact {
    let mut fact = SupplierRefundFact::new(
        SupplierRefundFactId::new(id),
        SupplierRefundFactData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            connection_id: SupplierApiConnectionId::new("connection-1"),
            supplier_fulfillment_order_id: order_id.clone(),
            external_refund_no: format!("RF-{id}"),
            external_refund_version: "1".to_string(),
            refund_amount: Amount::from_str(amount).unwrap(),
            refunded_at: Instant::from_unix_secs(1_700_000_000),
            source_event_id: format!("event-{id}"),
            inbox_message_id: InboxMessageId::new(format!("inbox-{id}")),
        },
    )
    .expect("退款事实构造失败");
    if deleted {
        fact.base.deleted_at = 1_700_000_001;
    }
    fact
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

/// FUL-R03 六态逐一及混合累计；软删动作头、软删动作行、软删申请行与
/// 软删明细一律排除；item_ids 与申请行限额只含未删除行。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn action_scope_counts_all_six_statuses_and_excludes_soft_deleted() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_action_scope_six_states")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let order_id = SupplierFulfillmentOrderId::new("order-six-states");
        let request_id = MallAfterSalesRequestId::new("request-six-states");
        let item_a = SupplierFulfillmentItemId::new("item-a");
        let request_line_a = MallAfterSalesRequestLineId::new("req-line-a");

        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_many(vec![
            fulfillment_item("item-a", &order_id, "100.0000", "1.000000", false),
            fulfillment_item("item-del", &order_id, "100.0000", "1.000000", true),
        ])
        .await
        .expect("明细插入失败");
        db.collection::<MallAfterSalesRequestLine>(
            <mongodb::Database as MallAfterSalesExt>::MALL_AFTER_SALES_REQUEST_LINES,
        )
        .insert_many(vec![
            request_line(
                "req-line-a",
                &request_id,
                &item_a,
                1,
                "10.000000",
                "100.00",
                false,
            ),
            request_line(
                "req-line-del",
                &request_id,
                &SupplierFulfillmentItemId::new("item-a"),
                2,
                "10.000000",
                "100.00",
                true,
            ),
        ])
        .await
        .expect("申请行插入失败");

        // 六态逐一：每种状态一个动作头，各带一行，数量/金额各不相同。
        // 混合累计：同一 scope 查询必须全部计入。
        let six_states = [
            (SupplierOrderActionStatus::Pending, "1.000000", "10.00"),
            (SupplierOrderActionStatus::Sending, "2.000000", "20.00"),
            (SupplierOrderActionStatus::ResultUnknown, "3.000000", "30.00"),
            (SupplierOrderActionStatus::Succeeded, "4.000000", "40.00"),
            (SupplierOrderActionStatus::Failed, "5.000000", "50.00"),
            (SupplierOrderActionStatus::Manual, "6.000000", "60.00"),
        ];
        let mut actions = Vec::new();
        let mut lines = Vec::new();
        for (index, (status, quantity, amount)) in six_states.into_iter().enumerate() {
            let action_id = SupplierOrderActionId::new(format!("action-{index}"));
            actions.push(action(
                &format!("action-{index}"),
                &order_id,
                &request_id,
                status,
                false,
            ));
            lines.push(action_line(
                &format!("action-{index}-line"),
                &action_id,
                &request_line_a,
                &item_a,
                quantity,
                amount,
                false,
            ));
        }
        // 软删动作头（其行必须一并排除）。
        let deleted_action_id = SupplierOrderActionId::new("action-del");
        actions.push(action(
            "action-del",
            &order_id,
            &request_id,
            SupplierOrderActionStatus::Succeeded,
            true,
        ));
        lines.push(action_line(
            "action-del-line",
            &deleted_action_id,
            &request_line_a,
            &item_a,
            "99.000000",
            "990.00",
            false,
        ));
        // 存活动作头下的软删动作行（行必须排除）。
        let live_action_id = SupplierOrderActionId::new("action-live");
        actions.push(action(
            "action-live",
            &order_id,
            &request_id,
            SupplierOrderActionStatus::Succeeded,
            false,
        ));
        lines.push(action_line(
            "action-live-line-del",
            &live_action_id,
            &request_line_a,
            &item_a,
            "88.000000",
            "880.00",
            true,
        ));
        db.collection::<SupplierOrderAction>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTIONS,
        )
        .insert_many(actions)
        .await
        .expect("动作头插入失败");
        db.collection::<SupplierOrderActionLine>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTION_LINES,
        )
        .insert_many(lines)
        .await
        .expect("动作行插入失败");

        let scope = db
            .supplier_fulfillment()
            .after_sales_action_scope(&order_id, &request_id, &mut NoTransaction)
            .await
            .expect("动作范围查询失败");

        assert_eq!(scope.item_ids, vec![item_a], "软删除明细不得出现在合法 item IDs");
        assert_eq!(
            scope
                .request_line_limits
                .iter()
                .map(|row| row.id.clone())
                .collect::<Vec<_>>(),
            vec![request_line_a.clone()],
            "软删除申请行不得出现在限额里"
        );
        let totals = scope
            .submitted_by_request_line
            .get(&request_line_a)
            .expect("六态动作行必须全部计入");
        assert_eq!(
            totals.quantity,
            Quantity::from_str("21.000000").expect("合法数量"),
            "1+2+3+4+5+6：六态逐一累计"
        );
        assert_eq!(
            totals.amount,
            Amount::from_str("210.00").expect("合法金额"),
            "10+20+30+40+50+60：混合累计"
        );
        assert_eq!(
            scope.submitted_by_request_line.len(),
            1,
            "软删动作头与软删动作行的金额不得串入累计"
        );
    });
}

/// FUL-R03 不同订单或申请不得串入：item_ids、申请行限额与已提交合计
/// 都只属于目标订单与目标申请。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn action_scope_isolates_orders_and_requests() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_action_scope_isolation")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let order_one = SupplierFulfillmentOrderId::new("order-iso-1");
        let order_two = SupplierFulfillmentOrderId::new("order-iso-2");
        let request_one = MallAfterSalesRequestId::new("request-iso-1");
        let request_two = MallAfterSalesRequestId::new("request-iso-2");
        let item_one = SupplierFulfillmentItemId::new("item-iso-1");
        let item_two = SupplierFulfillmentItemId::new("item-iso-2");
        let line_one = MallAfterSalesRequestLineId::new("req-line-iso-1");
        let line_two = MallAfterSalesRequestLineId::new("req-line-iso-2");

        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_many(vec![
            fulfillment_item("item-iso-1", &order_one, "100.0000", "1.000000", false),
            fulfillment_item("item-iso-2", &order_two, "200.0000", "1.000000", false),
        ])
        .await
        .expect("明细插入失败");
        db.collection::<MallAfterSalesRequestLine>(
            <mongodb::Database as MallAfterSalesExt>::MALL_AFTER_SALES_REQUEST_LINES,
        )
        .insert_many(vec![
            request_line(
                "req-line-iso-1",
                &request_one,
                &item_one,
                1,
                "10.000000",
                "100.00",
                false,
            ),
            request_line(
                "req-line-iso-2",
                &request_two,
                &item_two,
                1,
                "10.000000",
                "100.00",
                false,
            ),
        ])
        .await
        .expect("申请行插入失败");

        let action_one = SupplierOrderActionId::new("action-iso-1");
        let action_two = SupplierOrderActionId::new("action-iso-2");
        db.collection::<SupplierOrderAction>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTIONS,
        )
        .insert_many(vec![
            action(
                "action-iso-1",
                &order_one,
                &request_one,
                SupplierOrderActionStatus::Succeeded,
                false,
            ),
            action(
                "action-iso-2",
                &order_two,
                &request_two,
                SupplierOrderActionStatus::Failed,
                false,
            ),
        ])
        .await
        .expect("动作头插入失败");
        db.collection::<SupplierOrderActionLine>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTION_LINES,
        )
        .insert_many(vec![
            action_line(
                "action-iso-1-line",
                &action_one,
                &line_one,
                &item_one,
                "1.000000",
                "10.00",
                false,
            ),
            action_line(
                "action-iso-2-line",
                &action_two,
                &line_two,
                &item_two,
                "5.000000",
                "50.00",
                false,
            ),
        ])
        .await
        .expect("动作行插入失败");

        let scope_one = db
            .supplier_fulfillment()
            .after_sales_action_scope(&order_one, &request_one, &mut NoTransaction)
            .await
            .expect("范围一查询失败");
        assert_eq!(scope_one.item_ids, vec![item_one], "其他订单明细不得串入");
        assert_eq!(
            scope_one
                .request_line_limits
                .iter()
                .map(|row| row.id.clone())
                .collect::<Vec<_>>(),
            vec![line_one.clone()],
            "其他申请行不得串入限额"
        );
        let totals_one = scope_one
            .submitted_by_request_line
            .get(&line_one)
            .expect("范围一累计必须存在");
        assert_eq!(
            totals_one.quantity,
            Quantity::from_str("1.000000").expect("合法数量")
        );
        assert_eq!(totals_one.amount, Amount::from_str("10.00").expect("合法金额"));
        assert_eq!(
            scope_one.submitted_by_request_line.len(),
            1,
            "其他申请的动作行不得串入累计"
        );

        let scope_two = db
            .supplier_fulfillment()
            .after_sales_action_scope(&order_two, &request_two, &mut NoTransaction)
            .await
            .expect("范围二查询失败");
        assert_eq!(scope_two.item_ids, vec![item_two]);
        let totals_two = scope_two
            .submitted_by_request_line
            .get(&line_two)
            .expect("范围二累计必须存在");
        assert_eq!(
            totals_two.quantity,
            Quantity::from_str("5.000000").expect("合法数量")
        );
        assert_eq!(totals_two.amount, Amount::from_str("50.00").expect("合法金额"));
    });
}

/// FUL-R03 无历史动作：提交合计映射为空（Service 侧按精确零处理）；
/// 代表性 explain 证明动作头按申请查询命中新增的
/// `idx_supplier_order_actions_request` 索引。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn action_scope_without_history_returns_empty_map_and_uses_request_index() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_action_scope_no_history")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let order_id = SupplierFulfillmentOrderId::new("order-no-history");
        let request_id = MallAfterSalesRequestId::new("request-no-history");
        let item_a = SupplierFulfillmentItemId::new("item-no-history");

        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_one(fulfillment_item(
            "item-no-history",
            &order_id,
            "100.0000",
            "1.000000",
            false,
        ))
        .await
        .expect("明细插入失败");
        db.collection::<MallAfterSalesRequestLine>(
            <mongodb::Database as MallAfterSalesExt>::MALL_AFTER_SALES_REQUEST_LINES,
        )
        .insert_one(request_line(
            "req-line-no-history",
            &request_id,
            &item_a,
            1,
            "10.000000",
            "100.00",
            false,
        ))
        .await
        .expect("申请行插入失败");

        let scope = db
            .supplier_fulfillment()
            .after_sales_action_scope(&order_id, &request_id, &mut NoTransaction)
            .await
            .expect("动作范围查询失败");
        assert_eq!(scope.item_ids, vec![item_a]);
        assert_eq!(scope.request_line_limits.len(), 1);
        assert!(
            scope.submitted_by_request_line.is_empty(),
            "无历史动作必须返回空映射（Service 按精确零处理）"
        );

        // 代表性 explain：动作头按申请枚举查询命中 FUL-R03 新索引。
        let explain = db
            .run_command(doc! {
                "explain": {
                    "find": <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTIONS,
                    "filter": {
                        "after_sales_request_id": request_id.to_string(),
                        "deleted_at": 0i64,
                    },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("explain 失败");
        assert!(
            explain_uses_index(&Bson::Document(explain), "idx_supplier_order_actions_request"),
            "按申请枚举动作头必须命中 idx_supplier_order_actions_request"
        );
    });
}

/// FUL-R05 多明细成本、多退款事实正确累加；已删除明细与已删除退款事实
/// 不计入；金额精度精确到分；退款合计等于订单成本（全额边界）时两金额
/// 精确相等。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn refund_snapshot_accumulates_items_and_facts_excluding_deleted() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_refund_snapshot_accumulate")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let order_id = SupplierFulfillmentOrderId::new("order-accumulate");
        let other_order_id = SupplierFulfillmentOrderId::new("order-other");

        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_many(vec![
            fulfillment_item("item-acc-1", &order_id, "100.0000", "1.000000", false),
            fulfillment_item("item-acc-2", &order_id, "200.0000", "1.000000", false),
            fulfillment_item("item-acc-del", &order_id, "50.0000", "1.000000", true),
            fulfillment_item("item-acc-other", &other_order_id, "999.0000", "1.000000", false),
        ])
        .await
        .expect("明细插入失败");
        db.collection::<SupplierRefundFact>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
        )
        .insert_many(vec![
            refund_fact("fact-acc-1", &order_id, "10.00", false),
            refund_fact("fact-acc-2", &order_id, "5.50", false),
            refund_fact("fact-acc-del", &order_id, "99.00", true),
            refund_fact("fact-acc-other", &other_order_id, "999.00", false),
        ])
        .await
        .expect("退款事实插入失败");

        let snapshot = db
            .supplier_fulfillment()
            .refund_financial_snapshot(&order_id, &mut NoTransaction)
            .await
            .expect("财务快照查询失败");
        assert_eq!(
            snapshot.order_cost_gross,
            Amount::from_str("300.00").expect("合法金额"),
            "100.00 + 200.00：多明细成本累加，已删除明细不计入"
        );
        assert_eq!(
            snapshot.refunded_total,
            Amount::from_str("15.50").expect("合法金额"),
            "10.00 + 5.50：多退款事实累加，已删除与其他订单事实不计入"
        );

        // 全额退款边界：refunded_total == order_cost_gross 精确相等。
        let equal_order_id = SupplierFulfillmentOrderId::new("order-equal");
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_one(fulfillment_item(
            "item-equal",
            &equal_order_id,
            "100.0000",
            "1.000000",
            false,
        ))
        .await
        .expect("明细插入失败");
        db.collection::<SupplierRefundFact>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
        )
        .insert_one(refund_fact("fact-equal", &equal_order_id, "100.00", false))
        .await
        .expect("退款事实插入失败");
        let equal_snapshot = db
            .supplier_fulfillment()
            .refund_financial_snapshot(&equal_order_id, &mut NoTransaction)
            .await
            .expect("财务快照查询失败");
        assert_eq!(
            equal_snapshot.order_cost_gross, equal_snapshot.refunded_total,
            "退款合计等于订单成本时精确相等（Service 据此判定全额退款）"
        );
        assert_eq!(
            equal_snapshot.order_cost_gross,
            Amount::from_str("100.00").expect("合法金额")
        );
    });
}

/// FUL-R05 空集合为精确零：无明细且无退款事实时两个金额均为 `0.00`；
/// 有明细无退款事实时退款合计为零；无明细有退款事实时订单成本为零
/// （空明细按现行合同返回精确零金额）。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn refund_snapshot_empty_collections_are_exact_zero() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_refund_snapshot_empty")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let empty_order_id = SupplierFulfillmentOrderId::new("order-empty");
        let snapshot = db
            .supplier_fulfillment()
            .refund_financial_snapshot(&empty_order_id, &mut NoTransaction)
            .await
            .expect("空快照查询失败");
        assert_eq!(
            snapshot.order_cost_gross,
            Amount::from_str("0.00").expect("合法金额")
        );
        assert_eq!(
            snapshot.refunded_total,
            Amount::from_str("0.00").expect("合法金额")
        );

        // 有明细、无退款事实：退款合计精确零。
        let no_facts_order_id = SupplierFulfillmentOrderId::new("order-no-facts");
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_one(fulfillment_item(
            "item-no-facts",
            &no_facts_order_id,
            "100.0000",
            "1.000000",
            false,
        ))
        .await
        .expect("明细插入失败");
        let no_facts_snapshot = db
            .supplier_fulfillment()
            .refund_financial_snapshot(&no_facts_order_id, &mut NoTransaction)
            .await
            .expect("快照查询失败");
        assert_eq!(
            no_facts_snapshot.order_cost_gross,
            Amount::from_str("100.00").expect("合法金额")
        );
        assert_eq!(
            no_facts_snapshot.refunded_total,
            Amount::from_str("0.00").expect("合法金额"),
            "空退款必须为精确零"
        );

        // 无明细、有退款事实：订单成本精确零（空明细按现行合同处理）。
        let no_items_order_id = SupplierFulfillmentOrderId::new("order-no-items");
        db.collection::<SupplierRefundFact>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
        )
        .insert_one(refund_fact("fact-no-items", &no_items_order_id, "10.00", false))
        .await
        .expect("退款事实插入失败");
        let no_items_snapshot = db
            .supplier_fulfillment()
            .refund_financial_snapshot(&no_items_order_id, &mut NoTransaction)
            .await
            .expect("快照查询失败");
        assert_eq!(
            no_items_snapshot.order_cost_gross,
            Amount::from_str("0.00").expect("合法金额"),
            "空明细必须返回精确零而非空值"
        );
        assert_eq!(
            no_items_snapshot.refunded_total,
            Amount::from_str("10.00").expect("合法金额")
        );
    });
}

/// FUL-R03/FUL-R05 事务内读取与写入共用调用方 session：事务内未提交的
/// 动作头/动作行与退款事实对同一 session 的 scope/快照查询可见，提交后
/// 事务外查询结果一致。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn action_scope_and_refund_snapshot_use_caller_transaction_session() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_refund_scope_session")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let order_id = SupplierFulfillmentOrderId::new("order-tx-session");
        let request_id = MallAfterSalesRequestId::new("request-tx-session");
        let item_a = SupplierFulfillmentItemId::new("item-tx");
        let request_line_a = MallAfterSalesRequestLineId::new("req-line-tx");

        // 事务外先落明细与申请行（快照查询的事务外基线）。
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_one(fulfillment_item(
            "item-tx", &order_id, "100.0000", "1.000000", false,
        ))
        .await
        .expect("明细插入失败");
        db.collection::<MallAfterSalesRequestLine>(
            <mongodb::Database as MallAfterSalesExt>::MALL_AFTER_SALES_REQUEST_LINES,
        )
        .insert_one(request_line(
            "req-line-tx",
            &request_id,
            &item_a,
            1,
            "10.000000",
            "100.00",
            false,
        ))
        .await
        .expect("申请行插入失败");

        let client = db.client().clone();
        let db_in_tx = db.clone();
        let order_in_tx = order_id.clone();
        let request_in_tx = request_id.clone();
        let action_in_tx = action(
            "action-tx",
            &order_id,
            &request_id,
            SupplierOrderActionStatus::Succeeded,
            false,
        );
        let line_in_tx = action_line(
            "action-tx-line",
            &SupplierOrderActionId::new("action-tx"),
            &request_line_a,
            &item_a,
            "2.500000",
            "40.00",
            false,
        );
        let fact_in_tx = refund_fact("fact-tx", &order_id, "12.50", false);

        let (scope_in_tx, snapshot_in_tx) = client
            .with_transaction(move |session| {
                let db = db_in_tx.clone();
                let action = action_in_tx.clone();
                let line = line_in_tx.clone();
                let fact = fact_in_tx.clone();
                Box::pin(async move {
                    db.collection::<SupplierOrderAction>(
                        <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTIONS,
                    )
                    .insert_one(&action)
                    .session(&mut *session)
                    .await?;
                    db.collection::<SupplierOrderActionLine>(
                        <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTION_LINES,
                    )
                    .insert_one(&line)
                    .session(&mut *session)
                    .await?;
                    db.collection::<SupplierRefundFact>(
                        <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
                    )
                    .insert_one(&fact)
                    .session(&mut *session)
                    .await?;
                    let scope = db
                        .supplier_fulfillment()
                        .after_sales_action_scope(&order_in_tx, &request_in_tx, session)
                        .await?;
                    let snapshot = db
                        .supplier_fulfillment()
                        .refund_financial_snapshot(&order_in_tx, session)
                        .await?;
                    Ok::<_, database::Error>((scope, snapshot))
                })
            })
            .await
            .expect("事务内查询失败");

        let totals = scope_in_tx
            .submitted_by_request_line
            .get(&request_line_a)
            .expect("事务内未提交动作行必须可见");
        assert_eq!(
            totals.quantity,
            Quantity::from_str("2.500000").expect("合法数量"),
            "事务内聚合必须看到同一 session 的未提交动作行"
        );
        assert_eq!(totals.amount, Amount::from_str("40.00").expect("合法金额"));
        assert_eq!(
            snapshot_in_tx.order_cost_gross,
            Amount::from_str("100.00").expect("合法金额")
        );
        assert_eq!(
            snapshot_in_tx.refunded_total,
            Amount::from_str("12.50").expect("合法金额"),
            "事务内快照必须看到同一 session 的未提交退款事实"
        );

        // 提交后事务外查询结果一致。
        let scope_after = db
            .supplier_fulfillment()
            .after_sales_action_scope(&order_id, &request_id, &mut NoTransaction)
            .await
            .expect("提交后范围查询失败");
        let totals_after = scope_after
            .submitted_by_request_line
            .get(&request_line_a)
            .expect("提交后累计必须存在");
        assert_eq!(totals_after.quantity, totals.quantity);
        let snapshot_after = db
            .supplier_fulfillment()
            .refund_financial_snapshot(&order_id, &mut NoTransaction)
            .await
            .expect("提交后快照查询失败");
        assert_eq!(snapshot_after.refunded_total, snapshot_in_tx.refunded_total);
    });
}

/// 模拟 Service 退款登记的数据库错误分类（净余额校验是 Service 的跨聚合
/// 决定，测试只复刻其编排以验证仓储快照 + 订单 CAS 的资金并发闭环）。
#[derive(Debug)]
enum RefundAttemptError {
    /// 模拟 Service 的「历史累计 + 本次退款超过订单成本」拒绝。
    OverLimit,
    /// 数据库错误（含事务写冲突与唯一键冲突）。
    Db(database::Error),
}

impl From<database::Error> for RefundAttemptError {
    fn from(error: database::Error) -> Self {
        Self::Db(error)
    }
}

/// 复刻 `record_refund_result` 的事务编排：同事务内读取财务快照、校验
/// 净余额、CAS 更新订单版本并写入退款事实；任一失败整个事务不可见。
async fn attempt_refund(
    client: &mongodb::Client,
    db: &Database,
    order_id: SupplierFulfillmentOrderId,
    order: SupplierFulfillmentOrder,
    fact: SupplierRefundFact,
) -> std::result::Result<(), RefundAttemptError> {
    client
        .with_transaction(move |session| {
            let db = db.clone();
            Box::pin(async move {
                let mut order = order;
                let snapshot = db
                    .supplier_fulfillment()
                    .refund_financial_snapshot(&order_id, session)
                    .await?;
                let total_after = snapshot.refunded_total.checked_add(fact.refund_amount);
                if total_after > snapshot.order_cost_gross {
                    return Err(RefundAttemptError::OverLimit);
                }
                db.supplier_fulfillment_orders()
                    .update(&mut order, session)
                    .await?;
                db.collection::<SupplierRefundFact>(
                    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
                )
                .insert_one(&fact)
                .session(&mut *session)
                .await
                .map_err(database::Error::from)?;
                Ok(())
            })
        })
        .await
}

/// FUL-R05 两个并发退款不能共同突破净可退余额：两笔各 60.00 的退款
/// 对订单成本 100.00 单独都合法，但并发提交时订单版本 CAS 使恰好一笔
/// 成功、另一笔事务整体失败关闭，最终退款合计仍不超过订单成本。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_refunds_cannot_exceed_net_refundable_balance() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_refund_concurrent")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let order_id = SupplierFulfillmentOrderId::new("order-concurrent");
        let supplier_id = SupplierAccountId::new("supplier-1");
        let order = order("order-concurrent", &supplier_id);
        db.collection::<SupplierFulfillmentOrder>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
        )
        .insert_one(&order)
        .await
        .expect("订单插入失败");
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_one(fulfillment_item(
            "item-concurrent",
            &order_id,
            "100.0000",
            "1.000000",
            false,
        ))
        .await
        .expect("明细插入失败");

        let client = db.client().clone();
        let (first, second) = tokio::join!(
            attempt_refund(
                &client,
                db,
                order_id.clone(),
                order.clone(),
                refund_fact("refund-a", &order_id, "60.00", false),
            ),
            attempt_refund(
                &client,
                db,
                order_id.clone(),
                order.clone(),
                refund_fact("refund-b", &order_id, "60.00", false),
            ),
        );
        assert_eq!(
            [first.is_ok(), second.is_ok()].iter().filter(|ok| **ok).count(),
            1,
            "两笔并发退款必须恰好一笔成功（另一笔因订单 CAS 或净余额校验失败关闭）"
        );

        // 败者失败原因必须是可向调用方区分的两类之一：净余额校验拒绝
        // （先提交者已占额度）或数据库错误（订单 CAS 写冲突使事务整体回滚）。
        let loser = if first.is_err() { first } else { second };
        let loser_error = loser.expect_err("恰好一笔并发退款必须失败");
        match loser_error {
            RefundAttemptError::OverLimit => {
                // 另一笔先提交后快照已含累计退款，模拟的 Service 校验直接拒绝。
            }
            RefundAttemptError::Db(error) => {
                assert!(!error.to_string().is_empty(), "并发败者必须携带数据库错误信息");
            }
        }

        let snapshot = db
            .supplier_fulfillment()
            .refund_financial_snapshot(&order_id, &mut NoTransaction)
            .await
            .expect("提交后快照查询失败");
        assert!(
            snapshot.refunded_total <= snapshot.order_cost_gross,
            "并发退款不得共同突破净可退余额"
        );
        assert_eq!(
            snapshot.refunded_total,
            Amount::from_str("60.00").expect("合法金额"),
            "恰好一笔退款登记成功"
        );
        let fact_count = db
            .collection::<SupplierRefundFact>(
                <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
            )
            .count_documents(doc! { "supplier_fulfillment_order_id": order_id.to_string() })
            .await
            .expect("事实计数失败");
        assert_eq!(fact_count, 1, "失败事务的退款事实必须整体不可见");
    });
}
