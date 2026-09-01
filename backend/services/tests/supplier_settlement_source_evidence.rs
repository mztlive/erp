//! FUL-R06 来源证据录入的 Service fail-closed 边界（真实 MongoDB 验收）。
//!
//! 驱动公开 `record_source_evidence` 端到端验证：期间内完成明细漏行、期间内退款
//! 分配对应明细漏行、行不属于订单、订单不属于供应商均拒绝且零写入；全覆盖命令
//! 成功入库并可按请求 ID 幂等恢复。范围查询的有界性与索引见 database 集成测试
//! `supplier_settlement_source_scope`；本文件只验证 Service 保留的完整性/归属
//! 校验与证据构造。

use std::str::FromStr;

use database::{ensure_indexes, NoTransaction, SupplierFulfillmentExt, SupplierSettlementExt};
use entities::common::time::Instant;
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
use mongodb::Database;
use services::audit::AuditActor;
use services::supplier_settlement::{
    RecordSettlementSourceEvidenceLineRequest, RecordSettlementSourceEvidenceRequest,
    SupplierSettlementService,
};
use services::Error;
use test_support::{require_mongo, TestDb};

const SUPPLIER: &str = "supplier-1";
const OTHER_SUPPLIER: &str = "supplier-2";

/// 解析 RFC3339 时刻为秒级时间戳（夹具统一用 `+08:00` 业务时区）。
fn instant(rfc3339: &str) -> Instant {
    Instant::from_unix_secs(chrono::DateTime::parse_from_rfc3339(rfc3339).unwrap().timestamp())
}

/// 已完成履约订单夹具。
fn completed_order(
    id: &str,
    supplier_id: &SupplierAccountId,
    completed_at: Instant,
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
    allocation_no: u32,
) -> SupplierRefundAllocation {
    SupplierRefundAllocation::new(
        SupplierRefundAllocationId::new(id),
        SupplierRefundAllocationData {
            supplier_refund_fact_id: fact_id.clone(),
            allocation_no,
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

/// 来源命令行夹具（费用与账单补证固定为零/合法三元组）。
fn line(item_id: &str, order_id: &str) -> RecordSettlementSourceEvidenceLineRequest {
    RecordSettlementSourceEvidenceLineRequest {
        supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(order_id),
        supplier_fulfillment_item_id: SupplierFulfillmentItemId::new(item_id),
        cancel_occurred_at: None,
        cancel_evidence_reference_id: None,
        evidence_reference_ids: vec!["bill://line".to_string()],
        freight_gross: Amount::from_str("0.00").unwrap(),
        freight_net: Amount::from_str("0.00").unwrap(),
        freight_tax: Amount::from_str("0.00").unwrap(),
        service_fee_gross: Amount::from_str("0.00").unwrap(),
        service_fee_net: Amount::from_str("0.00").unwrap(),
        service_fee_tax: Amount::from_str("0.00").unwrap(),
        supplier_billed_gross: Amount::from_str("127.69").unwrap(),
        supplier_billed_net: Amount::from_str("111.03").unwrap(),
        supplier_billed_tax: Amount::from_str("16.66").unwrap(),
    }
}

/// 来源证据命令夹具。
fn request(
    request_id: &str,
    supplier_id: &str,
    lines: Vec<RecordSettlementSourceEvidenceLineRequest>,
) -> RecordSettlementSourceEvidenceRequest {
    RecordSettlementSourceEvidenceRequest {
        request_id: request_id.to_string(),
        idempotency_key: format!("key-{request_id}"),
        supplier_id: SupplierAccountId::new(supplier_id),
        period_start: "2026-07-01".to_string(),
        period_end: "2026-07-31".to_string(),
        period_policy_id: "monthly".to_string(),
        period_policy_version: "1".to_string(),
        timezone: "Asia/Shanghai".to_string(),
        source_version: 1,
        external_bill_no: "BILL-1".to_string(),
        external_bill_version: "1".to_string(),
        external_bill_evidence_reference_id: "bill://1".to_string(),
        lines,
    }
}

fn actor() -> AuditActor {
    AuditActor::new(
        "finance-1".to_string(),
        "finance-1".to_string(),
        entities::AccountKind::Admin,
    )
}

/// 断言请求未被持久化（fail-closed 零写入）。
async fn assert_no_evidence_persisted(db: &Database, request_id: &str) {
    let persisted = db
        .supplier_settlement_source_evidence()
        .find_by_request_id(request_id, &mut NoTransaction)
        .await
        .expect("幂等读取失败");
    assert!(persisted.is_none(), "拒绝的请求不得留下来源证据: {request_id}");
}

fn business_logic_message(error: Error) -> String {
    match error {
        Error::BusinessLogicError(message) => message,
        other => panic!("期望 BusinessLogicError，得到 {other:?}"),
    }
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn record_source_evidence_rejects_omitted_in_period_facts() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_settlement_evidence_incomplete")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let supplier = SupplierAccountId::new(SUPPLIER);
        let service = SupplierSettlementService::new(db.clone());

        // 订单 A：期间内完成，两条明细；退款事实在期间内，两条明细都有分配。
        let order_a = completed_order("order-a", &supplier, instant("2026-07-15T12:00:00+08:00"));
        let items_a = vec![
            fulfillment_item("item-a1", &SupplierFulfillmentOrderId::new("order-a")),
            fulfillment_item("item-a2", &SupplierFulfillmentOrderId::new("order-a")),
        ];
        db.collection::<SupplierFulfillmentOrder>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
        )
        .insert_one(order_a)
        .await
        .expect("订单插入失败");
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_many(items_a)
        .await
        .expect("明细插入失败");
        let fact_r1 = refund_fact(
            "fact-r1",
            &supplier,
            &SupplierFulfillmentOrderId::new("order-a"),
            instant("2026-07-20T10:00:00+08:00"),
        );
        db.collection::<SupplierRefundFact>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
        )
        .insert_one(fact_r1)
        .await
        .expect("退款事实插入失败");
        db.collection::<SupplierRefundAllocation>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS,
        )
        .insert_many(vec![
            refund_allocation(
                "alloc-r1a",
                &SupplierRefundFactId::new("fact-r1"),
                &SupplierFulfillmentItemId::new("item-a1"),
                1,
            ),
            refund_allocation(
                "alloc-r1b",
                &SupplierRefundFactId::new("fact-r1"),
                &SupplierFulfillmentItemId::new("item-a2"),
                2,
            ),
        ])
        .await
        .expect("退款分配插入失败");

        // 故意只提交 item-a1：期间内完成与退款事实都要求 item-a2，必须 fail-closed。
        let error = service
            .record_source_evidence(
                request("req-incomplete", SUPPLIER, vec![line("item-a1", "order-a")]),
                &actor(),
            )
            .await
            .expect_err("漏掉期间内完成/退款明细必须拒绝");
        let message = business_logic_message(error);
        assert!(
            message.contains("SOURCE_EVIDENCE_INCOMPLETE") && message.contains("item-a2"),
            "错误必须列出遗漏明细: {message}"
        );
        assert_no_evidence_persisted(db, "req-incomplete").await;

        // 订单 B：期间外完成，退款事实在期间内只分配给 item-b1；只提交 item-b2
        // 时只有退款路径能枚举到 item-b1，仍必须 fail-closed。
        let order_b = completed_order("order-b", &supplier, instant("2026-06-01T00:00:00+08:00"));
        let items_b = vec![
            fulfillment_item("item-b1", &SupplierFulfillmentOrderId::new("order-b")),
            fulfillment_item("item-b2", &SupplierFulfillmentOrderId::new("order-b")),
        ];
        db.collection::<SupplierFulfillmentOrder>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
        )
        .insert_one(order_b)
        .await
        .expect("订单插入失败");
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_many(items_b)
        .await
        .expect("明细插入失败");
        let fact_r2 = refund_fact(
            "fact-r2",
            &supplier,
            &SupplierFulfillmentOrderId::new("order-b"),
            instant("2026-07-10T10:00:00+08:00"),
        );
        db.collection::<SupplierRefundFact>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
        )
        .insert_one(fact_r2)
        .await
        .expect("退款事实插入失败");
        db.collection::<SupplierRefundAllocation>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS,
        )
        .insert_one(refund_allocation(
            "alloc-r2",
            &SupplierRefundFactId::new("fact-r2"),
            &SupplierFulfillmentItemId::new("item-b1"),
            1,
        ))
        .await
        .expect("退款分配插入失败");

        let error = service
            .record_source_evidence(
                request(
                    "req-incomplete-refund",
                    SUPPLIER,
                    vec![line("item-b2", "order-b")],
                ),
                &actor(),
            )
            .await
            .expect_err("漏掉期间内退款分配对应明细必须拒绝");
        let message = business_logic_message(error);
        assert!(
            message.contains("SOURCE_EVIDENCE_INCOMPLETE") && message.contains("item-b1"),
            "退款路径遗漏必须列出明细: {message}"
        );
        assert_no_evidence_persisted(db, "req-incomplete-refund").await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn record_source_evidence_rejects_line_not_belonging_to_order() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_settlement_evidence_foreign")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let supplier = SupplierAccountId::new(SUPPLIER);
        let service = SupplierSettlementService::new(db.clone());

        // 两个供应商 1 的期间内完成订单，各自一条明细。
        db.collection::<SupplierFulfillmentOrder>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
        )
        .insert_many(vec![
            completed_order("order-a", &supplier, instant("2026-07-15T12:00:00+08:00")),
            completed_order("order-b", &supplier, instant("2026-07-16T12:00:00+08:00")),
        ])
        .await
        .expect("订单插入失败");
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_many(vec![
            fulfillment_item("item-a1", &SupplierFulfillmentOrderId::new("order-a")),
            fulfillment_item("item-b1", &SupplierFulfillmentOrderId::new("order-b")),
        ])
        .await
        .expect("明细插入失败");

        // 完整性已覆盖（两条明细都在命令内），但 item-b1 被错误挂到 order-a 下。
        let error = service
            .record_source_evidence(
                request(
                    "req-mismatch",
                    SUPPLIER,
                    vec![line("item-a1", "order-a"), line("item-b1", "order-a")],
                ),
                &actor(),
            )
            .await
            .expect_err("行不属于订单必须拒绝");
        let message = business_logic_message(error);
        assert!(
            message.contains("不属于订单") && message.contains("item-b1") && message.contains("order-a"),
            "错误必须指向行与订单不匹配: {message}"
        );
        assert_no_evidence_persisted(db, "req-mismatch").await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn record_source_evidence_rejects_foreign_supplier_order() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_settlement_evidence_foreign_supplier")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let supplier = SupplierAccountId::new(SUPPLIER);
        let other_supplier = SupplierAccountId::new(OTHER_SUPPLIER);
        let service = SupplierSettlementService::new(db.clone());

        // 本供应商有一条期间内完成订单（范围非空），另有其他供应商的期间内
        // 完成订单；跨供应商引用只能取回明细、取不回订单头，必须 fail-closed。
        db.collection::<SupplierFulfillmentOrder>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
        )
        .insert_many(vec![
            completed_order("order-a", &supplier, instant("2026-07-15T12:00:00+08:00")),
            completed_order(
                "order-other",
                &other_supplier,
                instant("2026-07-15T12:00:00+08:00"),
            ),
        ])
        .await
        .expect("订单插入失败");
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_many(vec![
            fulfillment_item("item-a1", &SupplierFulfillmentOrderId::new("order-a")),
            fulfillment_item("item-other", &SupplierFulfillmentOrderId::new("order-other")),
        ])
        .await
        .expect("明细插入失败");

        // 跨供应商引用必须 fail-closed（范围查询在供应商边界排除，Service 拒绝）。
        let error = service
            .record_source_evidence(
                request("req-foreign", SUPPLIER, vec![line("item-other", "order-other")]),
                &actor(),
            )
            .await
            .expect_err("其他供应商订单必须拒绝");
        assert!(
            matches!(error, Error::BusinessLogicError(_)),
            "跨供应商引用必须业务拒绝: {error:?}"
        );
        assert_no_evidence_persisted(db, "req-foreign").await;
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn record_source_evidence_accepts_full_coverage_and_is_idempotent() {
    require_mongo!(async {
        let fixture = TestDb::new("ful_settlement_evidence_accept")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();
        let supplier = SupplierAccountId::new(SUPPLIER);
        let service = SupplierSettlementService::new(db.clone());

        db.collection::<SupplierFulfillmentOrder>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
        )
        .insert_one(completed_order(
            "order-a",
            &supplier,
            instant("2026-07-15T12:00:00+08:00"),
        ))
        .await
        .expect("订单插入失败");
        db.collection::<SupplierFulfillmentItem>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        )
        .insert_many(vec![
            fulfillment_item("item-a1", &SupplierFulfillmentOrderId::new("order-a")),
            fulfillment_item("item-a2", &SupplierFulfillmentOrderId::new("order-a")),
        ])
        .await
        .expect("明细插入失败");
        db.collection::<SupplierRefundFact>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
        )
        .insert_one(refund_fact(
            "fact-r1",
            &supplier,
            &SupplierFulfillmentOrderId::new("order-a"),
            instant("2026-07-20T10:00:00+08:00"),
        ))
        .await
        .expect("退款事实插入失败");
        db.collection::<SupplierRefundAllocation>(
            <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS,
        )
        .insert_one(refund_allocation(
            "alloc-r1",
            &SupplierRefundFactId::new("fact-r1"),
            &SupplierFulfillmentItemId::new("item-a1"),
            1,
        ))
        .await
        .expect("退款分配插入失败");

        let req = request(
            "req-complete",
            SUPPLIER,
            vec![line("item-a1", "order-a"), line("item-a2", "order-a")],
        );
        let created = service
            .record_source_evidence(req.clone(), &actor())
            .await
            .expect("全覆盖命令必须成功");
        assert_eq!(created.line_count, 2);
        assert_eq!(created.request_id, "req-complete");
        assert_eq!(created.supplier_id, SUPPLIER);
        assert_eq!(created.period_start, "2026-07-01");
        assert_eq!(created.period_end, "2026-07-31");
        assert_eq!(created.source_version, 1);

        // 幂等恢复：同一命令重复提交返回原批次而不是冲突。
        let replayed = service
            .record_source_evidence(req.clone(), &actor())
            .await
            .expect("重复命令必须幂等恢复");
        assert_eq!(replayed.id, created.id);

        let persisted = db
            .supplier_settlement_source_evidence()
            .find_by_request_id("req-complete", &mut NoTransaction)
            .await
            .expect("持久化读取失败")
            .expect("来源证据必须已持久化");
        assert_eq!(persisted.lines.len(), 2);
        let first = persisted
            .lines
            .iter()
            .find(|value| value.supplier_fulfillment_item_id.as_ref() == "item-a1")
            .expect("item-a1 行必须存在");
        assert!(first
            .source_fact_types
            .contains(&entities::supplier_settlement::SettlementSourceFactType::FulfillmentCompleted));
        assert!(first
            .source_fact_types
            .contains(&entities::supplier_settlement::SettlementSourceFactType::RefundConfirmed));
        assert_eq!(first.refund_gross, Amount::from_str("10.00").unwrap());
        assert_eq!(first.refund_net, Amount::from_str("8.70").unwrap());
        assert_eq!(first.refund_tax, Amount::from_str("1.30").unwrap());
        assert_eq!(first.order_gross, Amount::from_str("113.00").unwrap());
        assert_eq!(
            first.erp_gross,
            Amount::from_str("103.00").unwrap(),
            "ERP 金额 = 订单 113.00 − 退款 10.00"
        );
    });
}
