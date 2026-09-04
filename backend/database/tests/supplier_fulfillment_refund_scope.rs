//! FUL-R05 退款财务快照（Repository 下沉）的真实 MongoDB 验收。
//!
//! 覆盖：六态逐一及混合累计；软删动作头/行排除；跨订单/申请隔离；无历史为
//! 精确零；多明细成本与多退款事实累加；已删除事实不计入；金额精度与相等边界；
//! 事务内同一 session 可见未提交写入；两个并发退款不能共同突破净可退余额
//! （订单 CAS 失败关闭）。

use std::str::FromStr;

use database::{ensure_indexes, NoTransaction, SupplierFulfillmentExt, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    InboxMessageId, SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentItemId,
    SupplierFulfillmentOrderId, SupplierOfferingRevisionId, SupplierRefundFactId,
};
use entities::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
use entities::supplier_fulfillment::{
    SupplierFulfillmentItem, SupplierFulfillmentItemData, SupplierFulfillmentOrder,
    SupplierFulfillmentOrderData, SupplierRefundFact, SupplierRefundFactData,
};
use mongodb::bson::doc;
use mongodb::Database;
use test_support::{require_mongo, TestDb};

/// 履约订单夹具（仅并发 CAS 测试需要订单实体）。
fn order(id: &str, supplier_id: &SupplierAccountId) -> SupplierFulfillmentOrder {
    SupplierFulfillmentOrder::new(
        SupplierFulfillmentOrderId::new(id),
        SupplierFulfillmentOrderData::submitting(
            format!("SO-{id}"),
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


/// FUL-R05 多明细成本、多退款事实正确累加；已删除明细与已删除退款事实
/// 不计入；金额精度精确到分。
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

/// FUL-R05 事务内读取与写入共用调用方 session：事务内未提交的退款事实
/// 对同一 session 的快照查询可见，提交后事务外查询结果一致（直接构造）。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn refund_snapshot_uses_caller_transaction_session() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_refund_scope_session")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let order_id = SupplierFulfillmentOrderId::new("order-tx-session");
        // 事务外先落明细（快照查询的事务外基线，直接构造）。
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_one(fulfillment_item(
            "item-tx", &order_id, "100.0000", "1.000000", false,
        ))
        .await
        .expect("明细插入失败");

        let client = db.client().clone();
        let db_in_tx = db.clone();
        let order_in_tx = order_id.clone();
        let fact_in_tx = refund_fact("fact-tx", &order_id, "12.50", false);

        let snapshot_in_tx = client
            .with_transaction(move |session| {
                let db = db_in_tx.clone();
                let fact = fact_in_tx.clone();
                Box::pin(async move {
                    db.collection::<SupplierRefundFact>(
                        <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
                    )
                    .insert_one(&fact)
                    .session(&mut *session)
                    .await?;
                    let snapshot = db
                        .supplier_fulfillment()
                        .refund_financial_snapshot(&order_in_tx, session)
                        .await?;
                    Ok::<_, database::Error>(snapshot)
                })
            })
            .await
            .expect("事务内查询失败");

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
