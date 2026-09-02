//! INT-R11/INT-R12 商城售后额度批量范围与并发 CAS 的真实 MongoDB 验收。
//!
//! 覆盖：空 ID 集合；请求内重复 ID 去重后批量读取；APPLY/REVERSE 历史净额；
//! 无历史精确零；缺失 entry／allocation；恢复关联事实图；事务内 session 可见性；
//! 两个并发退款争用同一原消费、两个并发恢复争用同一退款分配时，商城订单版本
//! CAS 恰好一笔成功且败者零写入。

use std::str::FromStr;

use database::{ensure_indexes, MallAfterSalesExt, MallOrderExt, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    MallBalanceRestorationAllocationId, MallBalanceRestorationId, MallCardInstanceId, MallConsumptionEntryId,
    MallOrderFactId, MallOrderId, MallOrderItemId, MallPaymentSourceId, MallRefundAllocationId, MallRefundId,
    MallRefundLineId,
};
use entities::mall_after_sales::{
    AllocationAction, MallBalanceRestorationAllocation, MallBalanceRestorationAllocationData, MallRefund,
    MallRefundAllocation, MallRefundAllocationData, MallRefundData, MallRefundLine, MallRefundLineData,
};
use entities::mall_order::{
    AttributionStatus, ConsumptionDirection, FulfillmentChain, MallConsumptionEntry,
    MallConsumptionEntryData, MallOrder, MallOrderData, MallPaymentSource, MallPaymentSourceData,
    PaymentSourceType,
};
use entities::money::{Amount, Quantity};
use mongodb::Database;
use test_support::{require_mongo, TestDb};

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

fn mall_order(id: &str) -> MallOrder {
    MallOrder::new(
        MallOrderId::new(id),
        MallOrderData {
            mall_id: "mall-a".to_string(),
            external_order_no: format!("SO-{id}"),
            payment_fact_id: MallOrderFactId::new(format!("pay-{id}")),
            mall_user_ref: "user-1".to_string(),
            source_customer_ref: None,
            customer_id: None,
            ordered_at: Instant::from_unix_secs(1_700_000_000),
            paid_at: Instant::from_unix_secs(1_700_000_001),
            gross_amount: amount("100.00"),
            discount_amount: amount("0.00"),
            freight_amount: amount("0.00"),
            paid_amount: amount("100.00"),
            fulfillment_chain: FulfillmentChain::ErpAutomated,
            attribution_status: AttributionStatus::Attributed,
            address_snapshot_encrypted: None,
        },
    )
    .expect("商城订单构造失败")
}

fn consumption_entry(id: &str, amount_value: &str) -> MallConsumptionEntry {
    // 唯一键 uk_mall_consumption_entries_fact_item_source：每条夹具用独立 fact/item。
    consumption_entry_with_keys(
        id,
        &format!("fact-{id}"),
        &format!("item-{id}"),
        "ps-1",
        amount_value,
    )
}

fn consumption_entry_with_keys(
    id: &str,
    fact_id: &str,
    item_id: &str,
    payment_source_id: &str,
    amount_value: &str,
) -> MallConsumptionEntry {
    MallConsumptionEntry::new(
        MallConsumptionEntryId::new(id),
        MallConsumptionEntryData {
            mall_order_fact_id: MallOrderFactId::new(fact_id),
            mall_order_item_id: MallOrderItemId::new(item_id),
            mall_payment_source_id: MallPaymentSourceId::new(payment_source_id),
            direction: ConsumptionDirection::Consumption,
            amount: amount(amount_value),
            customer_id: None,
            origin_sales_order_id: None,
            sales_order_line_id: None,
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            attribution_status: AttributionStatus::Attributed,
            reverses_consumption_entry_id: None,
        },
    )
    .expect("消费事实构造失败")
}

/// 测试夹具：构造一条退款分配所需的最小字段集。
struct RefundAllocationFixture<'a> {
    id: &'a str,
    entry_id: &'a str,
    line_id: &'a str,
    amount_value: &'a str,
    action: AllocationAction,
    reverses: Option<&'a str>,
    reversal_entry: &'a str,
    allocation_no: u32,
}

/// 按夹具构造退款分配实体。
///
/// # 参数
/// * `fixture` - 退款分配测试字段
///
/// # 返回值
/// 构造成功的 `MallRefundAllocation`。
///
/// # 错误
/// 夹具字段非法时 panic（仅测试路径）。
fn refund_allocation(fixture: RefundAllocationFixture<'_>) -> MallRefundAllocation {
    MallRefundAllocation::new(
        MallRefundAllocationId::new(fixture.id),
        MallRefundAllocationData {
            mall_refund_line_id: MallRefundLineId::new(fixture.line_id),
            allocation_no: fixture.allocation_no,
            original_consumption_entry_id: MallConsumptionEntryId::new(fixture.entry_id),
            original_payment_source_id: MallPaymentSourceId::new("ps-1"),
            allocated_refund_amount: amount(fixture.amount_value),
            allocation_action: fixture.action,
            reverses_allocation_id: fixture.reverses.map(MallRefundAllocationId::new),
            reversal_consumption_entry_id: Some(MallConsumptionEntryId::new(fixture.reversal_entry)),
        },
    )
    .expect("退款分配构造失败")
}

fn refund_line(id: &str, refund_id: &str, line_no: u32) -> MallRefundLine {
    MallRefundLine::new(
        MallRefundLineId::new(id),
        MallRefundLineData {
            mall_refund_id: MallRefundId::new(refund_id),
            line_no,
            mall_order_item_id: MallOrderItemId::new("item-1"),
            refunded_quantity: Quantity::from_str("1.000000").unwrap(),
            line_refund_amount: amount("80.00"),
        },
    )
    .expect("退款行构造失败")
}

fn refund_header(id: &str) -> MallRefund {
    refund_header_with_request(id, "asr-1", "80.00")
}

fn refund_header_with_request(id: &str, request_id: &str, refund_amount: &str) -> MallRefund {
    MallRefund::new(
        MallRefundId::new(id),
        MallRefundData {
            mall_order_fact_id: MallOrderFactId::new(format!("refund-fact-{id}")),
            after_sales_request_id: entities::ids::MallAfterSalesRequestId::new(request_id),
            mall_id: "mall-a".to_string(),
            external_refund_no: format!("RF-{id}"),
            external_refund_version: "1".to_string(),
            mall_order_id: MallOrderId::new("order-1"),
            refund_amount: amount(refund_amount),
            refunded_at: Instant::from_unix_secs(1_700_000_100),
        },
    )
    .expect("退款头构造失败")
}

fn payment_source(id: &str) -> MallPaymentSource {
    MallPaymentSource::new(
        MallPaymentSourceId::new(id),
        MallPaymentSourceData {
            mall_order_id: MallOrderId::new("order-1"),
            source_no: 1,
            source_type: PaymentSourceType::Card,
            amount: amount("80.00"),
            source_card_instance_ref: Some("card-ref-1".to_string()),
            mall_card_instance_id: Some(MallCardInstanceId::new("card-1")),
            wechat_payment_ref: None,
            attribution_status: AttributionStatus::Attributed,
        },
    )
    .expect("支付来源构造失败")
}

fn restoration_allocation(
    id: &str,
    refund_allocation_id: &str,
    amount_value: &str,
    allocation_no: u32,
) -> MallBalanceRestorationAllocation {
    MallBalanceRestorationAllocation::new(
        MallBalanceRestorationAllocationId::new(id),
        MallBalanceRestorationAllocationData {
            mall_balance_restoration_id: MallBalanceRestorationId::new("br-1"),
            allocation_no,
            mall_refund_allocation_id: MallRefundAllocationId::new(refund_allocation_id),
            mall_card_instance_id: MallCardInstanceId::new("card-1"),
            restored_amount: amount(amount_value),
        },
    )
    .expect("恢复分配构造失败")
}

/// 空集合、重复 ID、APPLY/REVERSE 历史净额与缺失 entry。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn consumption_refund_limit_scope_batches_history_and_entries() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r11_refund_limit")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let empty = db
            .mall_after_sales()
            .consumption_refund_limit_scope(&[], &mut NoTransaction)
            .await
            .expect("空集合查询失败");
        assert!(empty.entries.is_empty());
        assert!(empty.historical_nets.is_empty());

        db.collection::<MallConsumptionEntry>(<mongodb::Database as MallOrderExt>::MALL_CONSUMPTION_ENTRIES)
            .insert_many(vec![
                consumption_entry("ce-1", "100.00"),
                consumption_entry("ce-2", "50.00"),
            ])
            .await
            .expect("消费事实插入失败");
        db.collection::<MallRefundAllocation>(
            <mongodb::Database as MallAfterSalesExt>::MALL_REFUND_ALLOCATIONS,
        )
        .insert_many(vec![
            refund_allocation(RefundAllocationFixture {
                id: "ra-1",
                entry_id: "ce-1",
                line_id: "rl-1",
                amount_value: "40.00",
                action: AllocationAction::Apply,
                reverses: None,
                reversal_entry: "rev-1",
                allocation_no: 1,
            }),
            refund_allocation(RefundAllocationFixture {
                id: "ra-2",
                entry_id: "ce-1",
                line_id: "rl-1",
                amount_value: "10.00",
                action: AllocationAction::Reverse,
                reverses: Some("ra-1"),
                reversal_entry: "rev-2",
                allocation_no: 2,
            }),
            refund_allocation(RefundAllocationFixture {
                id: "ra-3",
                entry_id: "ce-2",
                line_id: "rl-2",
                amount_value: "20.00",
                action: AllocationAction::Apply,
                reverses: None,
                reversal_entry: "rev-3",
                allocation_no: 1,
            }),
        ])
        .await
        .expect("退款分配插入失败");

        let ids = vec![
            MallConsumptionEntryId::new("ce-1"),
            MallConsumptionEntryId::new("ce-missing"),
            MallConsumptionEntryId::new("ce-1"),
            MallConsumptionEntryId::new("ce-2"),
        ];
        let scope = db
            .mall_after_sales()
            .consumption_refund_limit_scope(&ids, &mut NoTransaction)
            .await
            .expect("额度范围查询失败");
        assert_eq!(scope.entries.len(), 2);
        assert!(!scope
            .entries
            .contains_key(&MallConsumptionEntryId::new("ce-missing")));
        assert_eq!(
            scope
                .historical_nets
                .get(&MallConsumptionEntryId::new("ce-1"))
                .copied(),
            Some(amount("30.00"))
        );
        assert_eq!(
            scope
                .historical_nets
                .get(&MallConsumptionEntryId::new("ce-2"))
                .copied(),
            Some(amount("20.00"))
        );
    });
}

/// 恢复关联事实图、多次历史合计与空历史。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn restoration_limit_scope_loads_graph_and_history() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r12_restoration_limit")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        db.collection::<MallRefund>(<mongodb::Database as MallAfterSalesExt>::MALL_REFUNDS)
            .insert_one(refund_header("refund-1"))
            .await
            .expect("退款头插入失败");
        db.collection::<MallRefundLine>(<mongodb::Database as MallAfterSalesExt>::MALL_REFUND_LINES)
            .insert_one(refund_line("rl-1", "refund-1", 1))
            .await
            .expect("退款行插入失败");
        db.collection::<MallRefundAllocation>(
            <mongodb::Database as MallAfterSalesExt>::MALL_REFUND_ALLOCATIONS,
        )
        .insert_one(refund_allocation(RefundAllocationFixture {
            id: "ra-1",
            entry_id: "ce-1",
            line_id: "rl-1",
            amount_value: "80.00",
            action: AllocationAction::Apply,
            reverses: None,
            reversal_entry: "rev-1",
            allocation_no: 1,
        }))
        .await
        .expect("退款分配插入失败");
        db.collection::<MallPaymentSource>(<mongodb::Database as MallOrderExt>::MALL_PAYMENT_SOURCES)
            .insert_one(payment_source("ps-1"))
            .await
            .expect("支付来源插入失败");
        db.collection::<MallBalanceRestorationAllocation>(
            <mongodb::Database as MallAfterSalesExt>::MALL_BALANCE_RESTORATION_ALLOCATIONS,
        )
        .insert_many(vec![
            restoration_allocation("bra-1", "ra-1", "10.00", 1),
            restoration_allocation("bra-2", "ra-1", "15.00", 2),
        ])
        .await
        .expect("恢复分配插入失败");

        let scope = db
            .mall_after_sales()
            .restoration_limit_scope(
                &[
                    MallRefundAllocationId::new("ra-1"),
                    MallRefundAllocationId::new("ra-1"),
                    MallRefundAllocationId::new("ra-missing"),
                ],
                &mut NoTransaction,
            )
            .await
            .expect("恢复范围查询失败");
        assert_eq!(scope.refund_allocations.len(), 1);
        assert_eq!(scope.refund_lines.len(), 1);
        assert_eq!(scope.refunds.len(), 1);
        assert_eq!(scope.payment_sources.len(), 1);
        assert_eq!(
            scope
                .historical_restored
                .get(&MallRefundAllocationId::new("ra-1"))
                .copied(),
            Some(amount("25.00"))
        );
        assert!(!scope
            .refund_allocations
            .contains_key(&MallRefundAllocationId::new("ra-missing")));
    });
}

/// 模拟 Service 额度占用的错误分类。
#[derive(Debug)]
enum OccupancyAttemptError {
    /// 历史净额 + 本次超过原消费上限。
    OverLimit,
    /// 数据库错误（含订单 CAS 冲突）；断言侧只关心成败，载荷仅经 `From` 保留。
    Db(#[allow(dead_code)] database::Error),
}

impl From<database::Error> for OccupancyAttemptError {
    fn from(error: database::Error) -> Self {
        Self::Db(error)
    }
}

/// 复刻 Service 额度占用：先 CAS 商城订单，再在同一 session 重读净额并写入分配。
async fn attempt_refund_occupancy(
    client: &mongodb::Client,
    db: &Database,
    order: MallOrder,
    allocation: MallRefundAllocation,
    request_amount: Amount,
    entry_id: MallConsumptionEntryId,
    entry_limit: Amount,
) -> std::result::Result<(), OccupancyAttemptError> {
    client
        .with_transaction(move |session| {
            let db = db.clone();
            Box::pin(async move {
                let mut order = order;
                db.mall_orders().update(&mut order, session).await?;
                let scope = db
                    .mall_after_sales()
                    .consumption_refund_limit_scope(std::slice::from_ref(&entry_id), session)
                    .await?;
                let historical = scope
                    .historical_nets
                    .get(&entry_id)
                    .copied()
                    .unwrap_or_else(|| amount("0.00"));
                let accrued = historical.checked_add(request_amount);
                if accrued > entry_limit {
                    return Err(OccupancyAttemptError::OverLimit);
                }
                db.mall_refund_allocations().create(&allocation, session).await?;
                Ok(())
            })
        })
        .await
}

/// 两个并发退款争用同一原消费：各 60.00 vs 上限 100.00，CAS 使恰好一笔成功。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_refunds_on_same_entry_serialize_via_order_cas() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r11_concurrent")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let order = mall_order("order-concurrent");
        db.mall_orders()
            .create(&order, &mut NoTransaction)
            .await
            .expect("订单插入失败");
        db.collection::<MallConsumptionEntry>(<mongodb::Database as MallOrderExt>::MALL_CONSUMPTION_ENTRIES)
            .insert_one(consumption_entry("ce-concurrent", "100.00"))
            .await
            .expect("消费事实插入失败");

        let client = db.client().clone();
        let order_a = db
            .mall_orders()
            .find_by_id("order-concurrent", &mut NoTransaction)
            .await
            .expect("读订单失败")
            .expect("订单必须存在");
        let order_b = order_a.clone();
        let alloc_a = refund_allocation(RefundAllocationFixture {
            id: "ra-concurrent-a",
            entry_id: "ce-concurrent",
            line_id: "rl-a",
            amount_value: "60.00",
            action: AllocationAction::Apply,
            reverses: None,
            reversal_entry: "rev-a",
            allocation_no: 1,
        });
        let alloc_b = refund_allocation(RefundAllocationFixture {
            id: "ra-concurrent-b",
            entry_id: "ce-concurrent",
            line_id: "rl-b",
            amount_value: "60.00",
            action: AllocationAction::Apply,
            reverses: None,
            reversal_entry: "rev-b",
            allocation_no: 1,
        });

        let (result_a, result_b) = tokio::join!(
            attempt_refund_occupancy(
                &client,
                db,
                order_a,
                alloc_a,
                amount("60.00"),
                MallConsumptionEntryId::new("ce-concurrent"),
                amount("100.00"),
            ),
            attempt_refund_occupancy(
                &client,
                db,
                order_b,
                alloc_b,
                amount("60.00"),
                MallConsumptionEntryId::new("ce-concurrent"),
                amount("100.00"),
            ),
        );

        let success_count = [result_a.is_ok(), result_b.is_ok()]
            .into_iter()
            .filter(|ok| *ok)
            .count();
        assert_eq!(success_count, 1, "并发争用必须恰好一笔成功");

        let scope = db
            .mall_after_sales()
            .consumption_refund_limit_scope(
                &[MallConsumptionEntryId::new("ce-concurrent")],
                &mut NoTransaction,
            )
            .await
            .expect("并发后范围查询失败");
        assert_eq!(
            scope
                .historical_nets
                .get(&MallConsumptionEntryId::new("ce-concurrent"))
                .copied(),
            Some(amount("60.00")),
            "败者不得留下半写入"
        );
    });
}

/// 复刻 Service 恢复额度占用：先 CAS 商城订单，再在同一 session 重读净额并写入恢复分配。
async fn attempt_restoration_occupancy(
    client: &mongodb::Client,
    db: &Database,
    order: MallOrder,
    allocation: MallBalanceRestorationAllocation,
    request_amount: Amount,
    refund_allocation_id: MallRefundAllocationId,
    restoration_limit: Amount,
) -> std::result::Result<(), OccupancyAttemptError> {
    client
        .with_transaction(move |session| {
            let db = db.clone();
            Box::pin(async move {
                let mut order = order;
                db.mall_orders().update(&mut order, session).await?;
                let scope = db
                    .mall_after_sales()
                    .restoration_limit_scope(std::slice::from_ref(&refund_allocation_id), session)
                    .await?;
                let historical = scope
                    .historical_restored
                    .get(&refund_allocation_id)
                    .copied()
                    .unwrap_or_else(|| amount("0.00"));
                let accrued = historical.checked_add(request_amount);
                if accrued > restoration_limit {
                    return Err(OccupancyAttemptError::OverLimit);
                }
                db.mall_balance_restoration_allocations()
                    .create(&allocation, session)
                    .await?;
                Ok(())
            })
        })
        .await
}

/// 两个并发恢复争用同一退款分配：各 60.00 vs 上限 100.00，CAS 使恰好一笔成功。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_restorations_on_same_refund_allocation_serialize_via_order_cas() {
    require_mongo!(async {
        let fixture = TestDb::new("int_r12_concurrent")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let db = fixture.db();

        let order = mall_order("order-restore-concurrent");
        db.mall_orders()
            .create(&order, &mut NoTransaction)
            .await
            .expect("订单插入失败");
        db.collection::<MallRefund>(<mongodb::Database as MallAfterSalesExt>::MALL_REFUNDS)
            .insert_one(refund_header_with_request("refund-concurrent", "asr-1", "100.00"))
            .await
            .expect("退款头插入失败");
        db.collection::<MallRefundLine>(<mongodb::Database as MallAfterSalesExt>::MALL_REFUND_LINES)
            .insert_one(refund_line("rl-concurrent", "refund-concurrent", 1))
            .await
            .expect("退款行插入失败");
        db.collection::<MallRefundAllocation>(
            <mongodb::Database as MallAfterSalesExt>::MALL_REFUND_ALLOCATIONS,
        )
        .insert_one(refund_allocation(RefundAllocationFixture {
            id: "ra-concurrent",
            entry_id: "ce-1",
            line_id: "rl-concurrent",
            amount_value: "100.00",
            action: AllocationAction::Apply,
            reverses: None,
            reversal_entry: "rev-concurrent",
            allocation_no: 1,
        }))
        .await
        .expect("退款分配插入失败");

        let client = db.client().clone();
        let order_a = db
            .mall_orders()
            .find_by_id("order-restore-concurrent", &mut NoTransaction)
            .await
            .expect("读订单失败")
            .expect("订单必须存在");
        let order_b = order_a.clone();
        let alloc_a = restoration_allocation("bra-concurrent-a", "ra-concurrent", "60.00", 1);
        let alloc_b = restoration_allocation("bra-concurrent-b", "ra-concurrent", "60.00", 1);

        let (result_a, result_b) = tokio::join!(
            attempt_restoration_occupancy(
                &client,
                db,
                order_a,
                alloc_a,
                amount("60.00"),
                MallRefundAllocationId::new("ra-concurrent"),
                amount("100.00"),
            ),
            attempt_restoration_occupancy(
                &client,
                db,
                order_b,
                alloc_b,
                amount("60.00"),
                MallRefundAllocationId::new("ra-concurrent"),
                amount("100.00"),
            ),
        );

        let success_count = [result_a.is_ok(), result_b.is_ok()]
            .into_iter()
            .filter(|ok| *ok)
            .count();
        assert_eq!(success_count, 1, "并发恢复争用必须恰好一笔成功");

        let scope = db
            .mall_after_sales()
            .restoration_limit_scope(
                &[MallRefundAllocationId::new("ra-concurrent")],
                &mut NoTransaction,
            )
            .await
            .expect("并发后恢复范围查询失败");
        assert_eq!(
            scope
                .historical_restored
                .get(&MallRefundAllocationId::new("ra-concurrent"))
                .copied(),
            Some(amount("60.00")),
            "败者不得留下半写入"
        );
    });
}
