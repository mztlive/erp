//! 域 D29 `mall_order` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test mall_order_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::MallOrderExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    CustomerAccountId, InboxMessageId, MallConsumptionCostAssessmentId, MallConsumptionEntryId,
    MallItemFundingAllocationId, MallOrderCancelFactId, MallOrderCompletionFactId, MallOrderFactId,
    MallOrderId, MallOrderItemId, MallPaymentSourceId, SalesOrderId, SalesOrderLineId,
};
use entities::mall_order::consumption_entry::ConsumptionDirection;
use entities::mall_order::cost_assessment::CostBasisSourceType;
use entities::mall_order::types::{
    AttributionStatus, CancelScope, CostBasis, DataSource, FactType, FulfillmentChain, ProcessingStatus,
};
use entities::mall_order::{
    MallConsumptionCostAssessment, MallConsumptionCostAssessmentData, MallConsumptionEntry,
    MallConsumptionEntryData, MallItemFundingAllocation, MallItemFundingAllocationData, MallOrder,
    MallOrderCancelFact, MallOrderCancelFactData, MallOrderCompletionFact, MallOrderCompletionFactData,
    MallOrderData, MallOrderFact, MallOrderFactData, MallOrderItem, MallOrderItemData, MallPaymentSource,
    MallPaymentSourceData, PaymentSourceType,
};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 关键事实列表筛选条件类型（经 `MallOrderExt` 关联类型跨 crate 可达）。
type MallOrderFactFilter = <Database as MallOrderExt>::MallOrderFactFilter;
/// 商城订单列表筛选条件类型。
type MallOrderFilter = <Database as MallOrderExt>::MallOrderFilter;
/// 消费事实列表筛选条件类型。
type MallConsumptionEntryFilter = <Database as MallOrderExt>::MallConsumptionEntryFilter;

/// 构造可复用的支付成功事实实体。
fn sample_payment_fact(id: &str, business_key: &str) -> MallOrderFact {
    MallOrderFact::new(
        MallOrderFactId::new(id),
        MallOrderFactData {
            mall_id: format!(" {business_key} ").split(':').next().unwrap().to_string(),
            source_event_id: format!(" evt-{id} "),
            inbox_message_id: InboxMessageId::new(format!("inbox-{id}")),
            fact_type: FactType::PaymentSucceeded,
            business_fact_key: business_key.to_string(),
            external_order_no: " SO-1 ".to_string(),
            external_order_version: " v1 ".to_string(),
            after_sales_request_id: None,
            original_payment_fact_id: None,
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            received_at: Instant::from_unix_secs(1_700_000_100),
            data_source: DataSource::Realtime,
            raw_payload_reference: Some(" storage/payload-1 ".to_string()),
        },
    )
    .unwrap()
}

/// 构造可复用的商城订单实体。
fn sample_order(id: &str, fact_id: &str, order_no: &str) -> MallOrder {
    MallOrder::new(
        MallOrderId::new(id),
        MallOrderData {
            mall_id: "mall-a".to_string(),
            external_order_no: order_no.to_string(),
            payment_fact_id: MallOrderFactId::new(fact_id),
            mall_user_ref: "user-9".to_string(),
            source_customer_ref: Some("cust-9".to_string()),
            customer_id: None,
            ordered_at: Instant::from_unix_secs(1_699_999_900),
            paid_at: Instant::from_unix_secs(1_700_000_000),
            gross_amount: Amount::from_str("100.00").unwrap(),
            discount_amount: Amount::from_str("10.00").unwrap(),
            freight_amount: Amount::from_str("5.00").unwrap(),
            paid_amount: Amount::from_str("95.00").unwrap(),
            fulfillment_chain: FulfillmentChain::ErpAutomated,
            attribution_status: AttributionStatus::PendingAttribution,
            address_snapshot_encrypted: Some("<encrypted>".to_string()),
        },
    )
    .unwrap()
}

/// 构造可复用的商城订单明细实体。
fn sample_order_item(id: &str, order_id: &str) -> MallOrderItem {
    MallOrderItem::new(
        MallOrderItemId::new(id),
        MallOrderItemData {
            mall_order_id: MallOrderId::new(order_id),
            external_item_id: format!(" ext-{id} "),
            sku_id: Some(entities::ids::SkuId::new("sku-1")),
            product_publication_revision_id: None,
            supplier_offering_revision_id: None,
            name_snapshot: " 咖啡豆 1kg ".to_string(),
            spec_snapshot: None,
            quantity: Quantity::from_str("2.000000").unwrap(),
            unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
            line_gross_amount: Amount::from_str("19.98").unwrap(),
            allocated_discount_amount: Amount::from_str("0.98").unwrap(),
            allocated_freight_amount: Amount::from_str("1.00").unwrap(),
            paid_amount: Amount::from_str("20.00").unwrap(),
            sales_tax_rate: Rate::from_str("0.130000").unwrap(),
            unit_cost_snapshot: None,
            cost_snapshot_total: None,
            cost_tax_inclusion: None,
            cost_input_tax_rate: None,
        },
    )
    .unwrap()
}

/// 构造可复用的卡券支付来源实体。
fn sample_payment_source(id: &str, order_id: &str, source_no: u32) -> MallPaymentSource {
    MallPaymentSource::new(
        MallPaymentSourceId::new(id),
        MallPaymentSourceData {
            mall_order_id: MallOrderId::new(order_id),
            source_no,
            source_type: PaymentSourceType::Card,
            amount: Amount::from_str("80.00").unwrap(),
            source_card_instance_ref: Some(" ref-001 ".to_string()),
            mall_card_instance_id: Some(entities::ids::MallCardInstanceId::new("card-1")),
            wechat_payment_ref: None,
            attribution_status: AttributionStatus::PendingAttribution,
        },
    )
    .unwrap()
}

/// 构造可复用的分摊矩阵记录实体。
fn sample_allocation(id: &str, item_id: &str, source_id: &str) -> MallItemFundingAllocation {
    MallItemFundingAllocation::new(
        MallItemFundingAllocationId::new(id),
        MallItemFundingAllocationData {
            mall_order_item_id: MallOrderItemId::new(item_id),
            mall_payment_source_id: MallPaymentSourceId::new(source_id),
            allocated_payment_amount: Amount::from_str("80.00").unwrap(),
        },
    )
    .unwrap()
}

/// 构造可复用的消费事实实体。
fn sample_consumption_entry(id: &str, fact_id: &str, item_id: &str, source_id: &str) -> MallConsumptionEntry {
    MallConsumptionEntry::new(
        MallConsumptionEntryId::new(id),
        MallConsumptionEntryData {
            mall_order_fact_id: MallOrderFactId::new(fact_id),
            mall_order_item_id: MallOrderItemId::new(item_id),
            mall_payment_source_id: MallPaymentSourceId::new(source_id),
            direction: ConsumptionDirection::Consumption,
            amount: Amount::from_str("80.00").unwrap(),
            customer_id: Some(CustomerAccountId::new("cust-erp-1")),
            origin_sales_order_id: Some(SalesOrderId::new("so-1")),
            sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            attribution_status: AttributionStatus::PendingAttribution,
            reverses_consumption_entry_id: None,
        },
    )
    .unwrap()
}

/// 构造可复用的成本评估实体。
fn sample_cost_assessment(entry_id: &str, id: &str) -> MallConsumptionCostAssessment {
    MallConsumptionCostAssessment::new(
        MallConsumptionCostAssessmentId::new(id),
        MallConsumptionCostAssessmentData {
            mall_consumption_entry_id: MallConsumptionEntryId::new(entry_id),
            assessment_no: 1,
            cost_basis: CostBasis::Actual,
            basis_source_type: Some(CostBasisSourceType::MallCostSnapshot),
            basis_source_id: Some(" so-1 ".to_string()),
            basis_source_line_id: None,
            basis_source_version: Some(" v1 ".to_string()),
            source_snapshot_hash: Some(" 9f86d081 ".to_string()),
            gross_amount: Some(Amount::from_str("12.00").unwrap()),
            net_amount: Some(Amount::from_str("11.32").unwrap()),
            tax_amount: Some(Amount::from_str("0.68").unwrap()),
            tax_inclusion: Some(true),
            input_tax_rate: Some(Rate::from_str("0.060000").unwrap()),
            delta_cost_entry_id: None,
            supersedes_assessment_id: None,
            assessed_at: Instant::from_unix_secs(1_700_000_100),
            assessed_by: " cost-team ".to_string(),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as MallOrderExt>::MALL_ORDER_FACTS,
        &[
            "uk_mall_order_facts_business_key",
            "uk_mall_order_facts_inbox_message",
            "uk_mall_order_facts_source_event",
            "idx_mall_order_facts_status",
            "idx_mall_order_facts_after_sales_request",
        ],
    )
    .await
    .expect("mall_order_facts 索引缺失");
    assert_indexes(
        db,
        <Database as MallOrderExt>::MALL_ORDER_CANCEL_FACTS,
        &["uk_mall_order_cancel_facts_fact"],
    )
    .await
    .expect("mall_order_cancel_facts 索引缺失");
    assert_indexes(
        db,
        <Database as MallOrderExt>::MALL_ORDER_COMPLETION_FACTS,
        &["uk_mall_order_completion_facts_fact"],
    )
    .await
    .expect("mall_order_completion_facts 索引缺失");
    assert_indexes(
        db,
        <Database as MallOrderExt>::MALL_ORDERS,
        &[
            "uk_mall_orders_identity",
            "uk_mall_orders_payment_fact",
            "idx_mall_orders_customer_paid",
            "idx_mall_orders_fulfillment_paid",
        ],
    )
    .await
    .expect("mall_orders 索引缺失");
    assert_indexes(
        db,
        <Database as MallOrderExt>::MALL_ORDER_ITEMS,
        &["uk_mall_order_items_identity", "idx_mall_order_items_sku"],
    )
    .await
    .expect("mall_order_items 索引缺失");
    assert_indexes(
        db,
        <Database as MallOrderExt>::MALL_PAYMENT_SOURCES,
        &[
            "uk_mall_payment_sources_no",
            "idx_mall_payment_sources_card_instance",
        ],
    )
    .await
    .expect("mall_payment_sources 索引缺失");
    assert_indexes(
        db,
        <Database as MallOrderExt>::MALL_ITEM_FUNDING_ALLOCATIONS,
        &[
            "uk_mall_item_funding_allocations_cell",
            "idx_mall_item_funding_allocations_source",
        ],
    )
    .await
    .expect("mall_item_funding_allocations 索引缺失");
    assert_indexes(
        db,
        <Database as MallOrderExt>::MALL_CONSUMPTION_ENTRIES,
        &[
            "uk_mall_consumption_entries_fact_item_source",
            "idx_mall_consumption_entries_sales_order",
            "idx_mall_consumption_entries_status",
        ],
    )
    .await
    .expect("mall_consumption_entries 索引缺失");
    assert_indexes(
        db,
        <Database as MallOrderExt>::MALL_CONSUMPTION_COST_ASSESSMENTS,
        &[
            "uk_mall_consumption_cost_assessments_no",
            "uk_mall_consumption_cost_assessments_supersedes",
        ],
    )
    .await
    .expect("mall_consumption_cost_assessments 索引缺失");
}

#[tokio::test]
#[ignore]
async fn fact_create_and_read_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_fact_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let fact = sample_payment_fact("fact-1", "mall-a:PAYMENT:SO-1:v1");
        db.mall_order_facts()
            .create(&fact, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .mall_order_facts()
            .find_by_id(&fact.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.mall_id, "mall-a");
        assert_eq!(found.source_event_id, "evt-fact-1");
        assert_eq!(found.business_fact_key, "mall-a:PAYMENT:SO-1:v1");
        assert_eq!(found.external_order_no, "SO-1");
        assert_eq!(found.fact_type, FactType::PaymentSucceeded);
        assert_eq!(found.processing_status, ProcessingStatus::Saved);
        assert_eq!(found.occurred_at, Instant::from_unix_secs(1_700_000_000));
        assert_eq!(found.received_at, Instant::from_unix_secs(1_700_000_100));
        assert_eq!(found.raw_payload_reference.as_deref(), Some("storage/payload-1"));

        let by_key = db
            .mall_order_facts()
            .find_by_business_fact_key("mall-a:PAYMENT:SO-1:v1", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按业务事实键应命中");
        assert_eq!(by_key.base.id, "fact-1");

        let by_inbox = db
            .mall_order_facts()
            .find_by_inbox_message(&InboxMessageId::new("inbox-fact-1"), &mut NoTransaction)
            .await
            .unwrap()
            .expect("按共同信封应命中");
        assert_eq!(by_inbox.base.id, "fact-1");
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_business_fact_key_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_fact_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let fact = sample_payment_fact("fact-1", "mall-a:PAYMENT:SO-1:v1");
        db.mall_order_facts()
            .create(&fact, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_payment_fact("fact-2", "mall-a:PAYMENT:SO-1:v1");
        let error = db
            .mall_order_facts()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复 business_fact_key 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let same_source_event = sample_payment_fact("fact-3", "mall-a:PAYMENT:SO-2:v1");
        let mut same_source_event_clone = sample_payment_fact("fact-4", "mall-a:PAYMENT:SO-3:v1");
        same_source_event_clone.source_event_id = same_source_event.source_event_id.clone();
        db.mall_order_facts()
            .create(&same_source_event, &mut NoTransaction)
            .await
            .unwrap();
        let error = db
            .mall_order_facts()
            .create(&same_source_event_clone, &mut NoTransaction)
            .await
            .expect_err("同一 (商城, 来源事件) 重复必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn cancel_and_completion_facts_are_one_to_one_with_envelope() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_ext_1to1").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let cancel = MallOrderCancelFact::new(
            MallOrderCancelFactId::new("cf-1"),
            MallOrderCancelFactData {
                mall_order_fact_id: MallOrderFactId::new("fact-1"),
                cancel_version: " v2 ".to_string(),
                cancel_scope: CancelScope::WholeOrder,
                actual_canceled_quantity: Quantity::from_str("2.000000").unwrap(),
                actual_canceled_amount: Amount::from_str("199.00").unwrap(),
                reason: " 员工取消 ".to_string(),
            },
        )
        .unwrap();
        db.mall_order_cancel_facts()
            .create(&cancel, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(
            db.mall_order_cancel_facts()
                .find_by_fact_id(&MallOrderFactId::new("fact-1"), &mut NoTransaction)
                .await
                .unwrap()
                .unwrap()
                .cancel_version,
            "v2"
        );

        let second_cancel = MallOrderCancelFact::new(
            MallOrderCancelFactId::new("cf-2"),
            MallOrderCancelFactData {
                mall_order_fact_id: MallOrderFactId::new("fact-1"),
                cancel_version: "v3".to_string(),
                cancel_scope: CancelScope::LineItem,
                actual_canceled_quantity: Quantity::from_str("1.000000").unwrap(),
                actual_canceled_amount: Amount::from_str("99.50").unwrap(),
                reason: "部分取消".to_string(),
            },
        )
        .unwrap();
        let error = db
            .mall_order_cancel_facts()
            .create(&second_cancel, &mut NoTransaction)
            .await
            .expect_err("同一事实的第二个取消扩展必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let completion = MallOrderCompletionFact::new(
            MallOrderCompletionFactId::new("cp-1"),
            MallOrderCompletionFactData {
                mall_order_fact_id: MallOrderFactId::new("fact-2"),
                completion_version: " v5 ".to_string(),
                completed_at: Instant::from_unix_secs(1_700_000_300),
            },
        )
        .unwrap();
        db.mall_order_completion_facts()
            .create(&completion, &mut NoTransaction)
            .await
            .unwrap();
        let second_completion = MallOrderCompletionFact::new(
            MallOrderCompletionFactId::new("cp-2"),
            MallOrderCompletionFactData {
                mall_order_fact_id: MallOrderFactId::new("fact-2"),
                completion_version: "v6".to_string(),
                completed_at: Instant::from_unix_secs(1_700_000_400),
            },
        )
        .unwrap();
        let error = db
            .mall_order_completion_facts()
            .create(&second_completion, &mut NoTransaction)
            .await
            .expect_err("同一事实的第二个完成扩展必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn order_create_update_and_optimistic_lock() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_order_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order = sample_order("order-1", "fact-1", "SO-1");
        db.mall_orders().create(&order, &mut NoTransaction).await.unwrap();

        let found = db
            .mall_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.paid_amount, Amount::from_str("95.00").unwrap());
        assert_eq!(found.fulfillment_chain, FulfillmentChain::ErpAutomated);

        let mut stale = order.clone();
        order
            .update_attribution_status(AttributionStatus::Attributed)
            .unwrap();
        order.assign_customer(Some(CustomerAccountId::new("cust-erp-1")));
        db.mall_orders()
            .update(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(order.base.version, 2, "乐观锁成功后 version 递增");

        stale
            .update_attribution_status(AttributionStatus::Difference)
            .unwrap();
        let error = db
            .mall_orders()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn order_identity_and_payment_fact_uniqueness() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_order_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("order-1", "fact-1", "SO-1");
        db.mall_orders().create(&order, &mut NoTransaction).await.unwrap();

        let duplicate_identity = sample_order("order-2", "fact-2", "SO-1");
        let error = db
            .mall_orders()
            .create(&duplicate_identity, &mut NoTransaction)
            .await
            .expect_err("同一 (商城, 订单号) 重复订单必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let duplicate_fact = sample_order("order-3", "fact-1", "SO-2");
        let error = db
            .mall_orders()
            .create(&duplicate_fact, &mut NoTransaction)
            .await
            .expect_err("同一支付事实的第二份订单必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn consumption_entry_unique_combination_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_entry_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let entry = sample_consumption_entry("ce-1", "fact-1", "item-1", "ps-1");
        db.mall_consumption_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_consumption_entry("ce-2", "fact-1", "item-1", "ps-1");
        let error = db
            .mall_consumption_entries()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (事实, 明细, 来源, 方向) 重复消费必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let other_fact = sample_consumption_entry("ce-3", "fact-2", "item-1", "ps-1");
        db.mall_consumption_entries()
            .create(&other_fact, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            db.mall_consumption_entries()
                .find_by_id("ce-3", &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "不同事实同明细可共存"
        );
    })
}

#[tokio::test]
#[ignore]
async fn cost_assessment_roundtrip_and_chain_uniqueness() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_assessment").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let assessment = sample_cost_assessment("ce-1", "ca-1");
        db.mall_consumption_cost_assessments()
            .create(&assessment, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .mall_consumption_cost_assessments()
            .find_by_id(&assessment.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.gross_amount, Some(Amount::from_str("12.00").unwrap()));
        assert_eq!(found.net_amount, Some(Amount::from_str("11.32").unwrap()));
        assert_eq!(found.tax_amount, Some(Amount::from_str("0.68").unwrap()));
        assert_eq!(found.input_tax_rate, Some(Rate::from_str("0.060000").unwrap()));

        let duplicate = sample_cost_assessment("ce-1", "ca-2");
        let error = db
            .mall_consumption_cost_assessments()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同消费重复评估号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let chain = db
            .mall_consumption_cost_assessments()
            .list_by_entry(
                &entities::ids::MallConsumptionEntryId::new("ce-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].assessment_no, 1);
    })
}

#[tokio::test]
#[ignore]
async fn item_payment_source_and_allocations_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_items").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let item = sample_order_item("item-1", "order-1");
        db.mall_order_items()
            .create(&item, &mut NoTransaction)
            .await
            .unwrap();
        let item_2 = sample_order_item("item-2", "order-1");
        db.mall_order_items()
            .create(&item_2, &mut NoTransaction)
            .await
            .unwrap();

        let source = sample_payment_source("ps-1", "order-1", 1);
        db.mall_payment_sources()
            .create(&source, &mut NoTransaction)
            .await
            .unwrap();

        db.mall_item_funding_allocations()
            .create(&sample_allocation("ifa-1", "item-1", "ps-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.mall_item_funding_allocations()
            .create(&sample_allocation("ifa-2", "item-2", "ps-1"), &mut NoTransaction)
            .await
            .unwrap();

        let items = db
            .mall_order_items()
            .list_items_by_order(&MallOrderId::new("order-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].external_item_id, "ext-item-1");
        assert_eq!(items[0].paid_amount, Amount::from_str("20.00").unwrap());

        let sources = db
            .mall_payment_sources()
            .list_by_order(&MallOrderId::new("order-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_card_instance_ref.as_deref(), Some("ref-001"));
        assert!(
            db.mall_payment_sources()
                .list_by_card_instance(
                    &entities::ids::MallCardInstanceId::new("card-1"),
                    &mut NoTransaction,
                )
                .await
                .unwrap()
                .len()
                == 1,
            "按卡实例可追溯支付来源"
        );

        let allocations = db
            .mall_item_funding_allocations()
            .list_by_items(
                &[MallOrderItemId::new("item-1"), MallOrderItemId::new("item-2")],
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(allocations.len(), 2, "$in 批量取回两条分摊，无 N+1");
        assert!(
            db.mall_item_funding_allocations()
                .list_by_payment_source(&MallPaymentSourceId::new("ps-1"), &mut NoTransaction)
                .await
                .unwrap()
                .len()
                == 2,
            "按支付来源可追溯分摊"
        );
    })
}

#[tokio::test]
#[ignore]
async fn fact_projection_list_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_fact_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut fact_1 = sample_payment_fact("fact-1", "mall-a:PAYMENT:SO-1:v1");
        fact_1.processing_status = ProcessingStatus::PendingAttribution;
        db.mall_order_facts()
            .create(&fact_1, &mut NoTransaction)
            .await
            .unwrap();
        let mut fact_2 = sample_payment_fact("fact-2", "mall-b:PAYMENT:SO-2:v1");
        fact_2.processing_status = ProcessingStatus::Difference;
        db.mall_order_facts()
            .create(&fact_2, &mut NoTransaction)
            .await
            .unwrap();

        let filter = MallOrderFactFilter {
            mall_id: Some("mall-a".to_string()),
            fact_type: Some(FactType::PaymentSucceeded),
            processing_status: Some(ProcessingStatus::PendingAttribution),
            after_sales_request_id: None,
            page: 1,
            page_size: 10,
            sort_by: Some("occurred_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_order_facts()
            .search_facts(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.mall_id, "mall-a");
        assert_eq!(row.fact_type, FactType::PaymentSucceeded);
        assert_eq!(row.business_fact_key, "mall-a:PAYMENT:SO-1:v1");
        assert_eq!(row.external_order_no, "SO-1");
        assert_eq!(row.processing_status, ProcessingStatus::PendingAttribution);
        assert_eq!(row.occurred_at, Instant::from_unix_secs(1_700_000_000));

        let second_page = MallOrderFactFilter {
            page: 2,
            page_size: 1,
            ..filter
        };
        let empty = db
            .mall_order_facts()
            .search_facts(&second_page, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(empty.items.len(), 0, "分页边界：第二页为空");
    })
}

#[tokio::test]
#[ignore]
async fn order_projection_list_respects_regex_filters_and_time_range() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_order_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order_1 = sample_order("order-1", "fact-1", "SO-1");
        order_1.assign_customer(Some(CustomerAccountId::new("cust-erp-1")));
        order_1
            .update_attribution_status(AttributionStatus::Attributed)
            .unwrap();
        db.mall_orders()
            .create(&order_1, &mut NoTransaction)
            .await
            .unwrap();
        db.mall_orders()
            .create(&sample_order("order-2", "fact-2", "SO-2"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = MallOrderFilter {
            mall_id: None,
            external_order_no: Some("so-".to_string()),
            customer_id: Some(CustomerAccountId::new("cust-erp-1")),
            fulfillment_chain: None,
            attribution_status: Some(AttributionStatus::Attributed),
            paid_at_from: Some(Instant::from_unix_secs(1_699_999_990)),
            paid_at_to: None,
            page: 1,
            page_size: 10,
            sort_by: Some("paid_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .mall_orders()
            .search_orders(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "已归集且绑定客户且订单号匹配只有一条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.external_order_no, "SO-1");
        assert_eq!(row.customer_id, Some(CustomerAccountId::new("cust-erp-1")));
        assert_eq!(row.paid_amount, Amount::from_str("95.00").unwrap());
        assert_eq!(row.fulfillment_chain, FulfillmentChain::ErpAutomated);
        assert_eq!(row.attribution_status, AttributionStatus::Attributed);
        assert_eq!(row.paid_at, Instant::from_unix_secs(1_700_000_000));
        assert!(row.version >= 1);
    })
}

#[tokio::test]
#[ignore]
async fn consumption_entry_projection_list_filters_by_sales_order_and_direction() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_entry_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.mall_consumption_entries()
            .create(
                &sample_consumption_entry("ce-1", "fact-1", "item-1", "ps-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let mut other = sample_consumption_entry("ce-2", "fact-2", "item-2", "ps-2");
        other.origin_sales_order_id = Some(SalesOrderId::new("so-2"));
        db.mall_consumption_entries()
            .create(&other, &mut NoTransaction)
            .await
            .unwrap();

        let filter = MallConsumptionEntryFilter {
            origin_sales_order_id: Some(SalesOrderId::new("so-1")),
            direction: Some(ConsumptionDirection::Consumption),
            attribution_status: Some(AttributionStatus::PendingAttribution),
            occurred_at_from: None,
            occurred_at_to: None,
            page: 1,
            page_size: 10,
            sort_by: Some("occurred_at".to_string()),
            sort_ascending: false,
        };
        let page = db
            .mall_consumption_entries()
            .search_consumption_entries(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.items[0];
        assert_eq!(row.origin_sales_order_id, Some(SalesOrderId::new("so-1")));
        assert_eq!(row.direction, ConsumptionDirection::Consumption);
        assert_eq!(row.amount, Amount::from_str("80.00").unwrap());
        assert_eq!(
            row.mall_order_fact_id,
            entities::ids::MallOrderFactId::new("fact-1")
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_writes_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let fact = sample_payment_fact("fact-1", "mall-a:PAYMENT:SO-1:v1");
        let order = sample_order("order-1", "fact-1", "SO-1");

        let db_clone = db.clone();
        let fact_for_tx = fact.clone();
        let order_for_tx = order.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_order()
                        .create_payment_fact_with_order(&fact_for_tx, &order_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        assert!(
            db.mall_order_facts()
                .find_by_id(&fact.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "事务提交后事实必须可见"
        );
        assert!(
            db.mall_orders()
                .find_by_id(&order.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "事务提交后订单必须可见"
        );
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_both_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let fact = sample_payment_fact("fact-1", "mall-a:PAYMENT:SO-1:v1");
        let order = sample_order("order-1", "fact-1", "SO-1");

        let db_clone = db.clone();
        let fact_for_tx = fact.clone();
        let order_for_tx = order.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_order()
                        .create_payment_fact_with_order(&fact_for_tx, &order_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        assert!(
            db.mall_order_facts()
                .find_by_id(&fact.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后事实不得残留"
        );
        assert!(
            db.mall_orders()
                .find_by_id(&order.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后订单不得残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_conflict_rolls_back_whole_write() {
    require_mongo!(async {
        let test_db = TestDb::new("morder_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let existing = sample_payment_fact("fact-1", "mall-a:PAYMENT:SO-1:v1");
        db.mall_order_facts()
            .create(&existing, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate_fact = sample_payment_fact("fact-9", "mall-a:PAYMENT:SO-1:v1");
        let order = sample_order("order-9", "fact-9", "SO-9");

        let db_clone = db.clone();
        let duplicate_for_tx = duplicate_fact.clone();
        let order_for_tx = order.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .mall_order()
                        .create_payment_fact_with_order(&duplicate_for_tx, &order_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(database::Error::DuplicateKey(_))),
            "唯一冲突必须透出 DuplicateKey，实际为 {result:?}"
        );

        assert!(
            db.mall_orders()
                .find_by_id(&order.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "冲突回滚后订单不得残留"
        );
    })
}
