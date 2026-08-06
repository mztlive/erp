//! 域 D32 `supplier_fulfillment` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test supplier_fulfillment_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use std::str::FromStr;

use database::repository::extensions::SupplierFulfillmentExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::ids::{
    CostAllocationId, CostEntryId, InboxMessageId, MallOrderId, MallOrderItemId, PayableEntryId,
    PaymentAllocationId, SupplierAccountId, SupplierApiConnectionId, SupplierCatalogSkuId,
    SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierOfferingRevisionId, SupplierOrderActionId,
    SupplierOrderStatusHistoryId, SupplierRefundAllocationId, SupplierRefundFactId, SupplierRefundId,
};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use entities::supplier_fulfillment::{
    AllocationAction, CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentItem,
    SupplierFulfillmentItemData, SupplierFulfillmentOrder, SupplierFulfillmentOrderData,
    SupplierFulfillmentOrderUpdate, SupplierOrderAction, SupplierOrderActionData, SupplierOrderActionStatus,
    SupplierOrderActionType, SupplierOrderStatusHistory, SupplierOrderStatusHistoryData,
    SupplierRefundAllocation, SupplierRefundAllocationData, SupplierRefundFact, SupplierRefundFactData,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 供应商履约订单列表筛选条件类型（经 `SupplierFulfillmentExt` 关联类型跨 crate 可达）。
type SupplierFulfillmentOrderFilter = <Database as SupplierFulfillmentExt>::SupplierFulfillmentOrderFilter;

/// 构造可复用的供应商履约订单实体。
fn sample_order(no: &str) -> SupplierFulfillmentOrder {
    SupplierFulfillmentOrder::new(
        SupplierFulfillmentOrderId::new(format!("order-{no}")),
        SupplierFulfillmentOrderData {
            fulfillment_order_no: no.to_string(),
            mall_order_id: MallOrderId::new(format!("mall-order-{no}")),
            supplier_id: SupplierAccountId::new(format!("supplier-{no}")),
            connection_id: SupplierApiConnectionId::new("connection-1"),
            split_no: 1,
            fulfillment_status: FulfillmentStatus::Received,
            cancel_status: CancelStatus::None,
            refund_status: RefundStatus::None,
            external_order_no: None,
            submitted_at: None,
            accepted_at: None,
            completed_at: None,
            address_snapshot_encrypted: "encrypted-address".to_string(),
            address_snapshot_fingerprint: "fingerprint-address".to_string(),
        },
    )
    .unwrap()
}

/// 构造可复用的供应商履约明细实体（成本快照 3 × 9.99 = 29.97）。
fn sample_item(order_id: &SupplierFulfillmentOrderId, item_no: &str) -> SupplierFulfillmentItem {
    SupplierFulfillmentItem::new(
        SupplierFulfillmentItemId::new(format!("item-{item_no}")),
        SupplierFulfillmentItemData {
            supplier_fulfillment_order_id: order_id.clone(),
            mall_order_item_id: MallOrderItemId::new(format!("mall-item-{item_no}")),
            supplier_offering_revision_id: entities::ids::SupplierOfferingRevisionId::new(format!(
                "offering-{item_no}"
            )),
            supplier_catalog_sku_id: entities::ids::SupplierCatalogSkuId::new(format!("sku-{item_no}")),
            quantity: Quantity::from_str("3.000000").unwrap(),
            unit_cost_snapshot_gross: UnitPrice::from_str("9.9900").unwrap(),
            cost_snapshot_total_gross: Amount::from_str("29.97").unwrap(),
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
        },
    )
    .unwrap()
}

/// 构造可复用的首个 `PLACE` 动作（下单幂等键为 ERP 供应商订单号，§6.19）。
fn sample_place_action(order_id: &SupplierFulfillmentOrderId, no: &str) -> SupplierOrderAction {
    SupplierOrderAction::new(
        SupplierOrderActionId::new(format!("action-{no}")),
        SupplierOrderActionData {
            supplier_fulfillment_order_id: order_id.clone(),
            action_type: SupplierOrderActionType::Place,
            after_sales_request_id: None,
            idempotency_key: no.to_string(),
            status: SupplierOrderActionStatus::Pending,
            external_request_id: None,
            request_summary: None,
            response_summary: None,
            attempt_count: 0,
            next_attempt_at: None,
        },
    )
    .unwrap()
}

/// 构造可复用的状态历史记录（合法迁移 RECEIVED → SUBMITTING）。
fn sample_status_history(
    connection_id: &SupplierApiConnectionId,
    event_id: &str,
) -> SupplierOrderStatusHistory {
    SupplierOrderStatusHistory::new(
        SupplierOrderStatusHistoryId::new(format!("history-{event_id}")),
        SupplierOrderStatusHistoryData {
            connection_id: connection_id.clone(),
            previous_status: FulfillmentStatus::Received,
            new_status: FulfillmentStatus::Submitting,
            supplier_status_version: "v5".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            received_at: Instant::from_unix_secs(1_700_000_100),
            external_event_id: event_id.to_string(),
            source_type: SourceType::SupplierCallback,
        },
    )
    .unwrap()
}

/// 构造可复用的退款事实头与 APPLY 分配行（分配合计 = 退款头金额，§6.19）。
fn sample_refund(
    order_id: &SupplierFulfillmentOrderId,
    refund_no: &str,
) -> (SupplierRefundFact, SupplierRefundAllocation) {
    let fact = SupplierRefundFact::new(
        SupplierRefundFactId::new(format!("fact-{refund_no}")),
        SupplierRefundFactData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            connection_id: SupplierApiConnectionId::new("connection-1"),
            supplier_fulfillment_order_id: order_id.clone(),
            external_refund_no: refund_no.to_string(),
            external_refund_version: "1".to_string(),
            refund_amount: Amount::from_str("19.98").unwrap(),
            refunded_at: Instant::from_unix_secs(1_700_000_000),
            source_event_id: format!("EVT-{refund_no}"),
            inbox_message_id: InboxMessageId::new(format!("message-{refund_no}")),
        },
    )
    .unwrap();
    let allocation = SupplierRefundAllocation::new(
        SupplierRefundAllocationId::new(format!("allocation-{refund_no}")),
        SupplierRefundAllocationData {
            supplier_refund_fact_id: fact.base.id.clone().into(),
            allocation_no: 1,
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            original_cost_entry_id: CostEntryId::new("cost-entry-1"),
            original_cost_allocation_id: CostAllocationId::new("cost-allocation-1"),
            original_payable_entry_id: PayableEntryId::new("payable-entry-1"),
            original_payment_allocation_id: Some(PaymentAllocationId::new("payment-alloc-1")),
            refund_quantity: Quantity::from_str("2.000000").unwrap(),
            gross_amount: Amount::from_str("19.98").unwrap(),
            net_amount: Amount::from_str("17.38").unwrap(),
            tax_amount: Amount::from_str("2.60").unwrap(),
            payable_reduction_amount: Amount::from_str("9.99").unwrap(),
            cash_refund_amount: Amount::from_str("9.99").unwrap(),
            cash_supplier_refund_id: Some(SupplierRefundId::new("cash-refund-1")),
            allocation_action: AllocationAction::Apply,
            reverses_allocation_id: None,
        },
    )
    .unwrap();
    (fact, allocation)
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
        &[
            "uk_supplier_fulfillment_orders_order_no",
            "uk_supplier_fulfillment_orders_mall_supplier_split",
            "uk_supplier_fulfillment_orders_connection_external",
            "idx_supplier_fulfillment_orders_supplier_status_created",
            "idx_supplier_fulfillment_orders_external_order_no",
            "idx_supplier_fulfillment_orders_mall_order",
        ],
    )
    .await
    .expect("supplier_fulfillment_orders 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
        &[
            "uk_supplier_fulfillment_items_mall_order_item",
            "idx_supplier_fulfillment_items_order",
        ],
    )
    .await
    .expect("supplier_fulfillment_items 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTIONS,
        &[
            "uk_supplier_order_actions_idempotency_key",
            "idx_supplier_order_actions_order",
        ],
    )
    .await
    .expect("supplier_order_actions 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTION_LINES,
        &[
            "uk_supplier_order_action_lines_action_line_no",
            "uk_supplier_order_action_lines_action_request_line",
        ],
    )
    .await
    .expect("supplier_order_action_lines 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_STATUS_HISTORIES,
        &["uk_supplier_order_status_histories_connection_event"],
    )
    .await
    .expect("supplier_order_status_histories 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
        &[
            "uk_supplier_refund_facts_connection_refund",
            "uk_supplier_refund_facts_inbox_message",
            "idx_supplier_refund_facts_order",
        ],
    )
    .await
    .expect("supplier_refund_facts 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS,
        &[
            "uk_supplier_refund_allocations_fact_no",
            "uk_supplier_refund_allocations_reverse_source",
        ],
    )
    .await
    .expect("supplier_refund_allocations 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_read_roundtrip_preserves_money_and_statuses() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("FO-1");
        let order_id = order.base.id.clone().into();
        let item = sample_item(&order_id, "1");
        let action = sample_place_action(&order_id, "FO-1");

        let db_clone = db.clone();
        let order_for_tx = order.clone();
        let item_for_tx = item.clone();
        let action_for_tx = action.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_fulfillment()
                        .create_fulfillment_with_items_and_place_action(
                            &order_for_tx,
                            &[item_for_tx],
                            &action_for_tx,
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let found = db
            .supplier_fulfillment_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.fulfillment_order_no, "FO-1");
        assert_eq!(found.fulfillment_status, FulfillmentStatus::Received);
        assert_eq!(found.cancel_status, CancelStatus::None);
        assert_eq!(found.refund_status, RefundStatus::None);

        let items = db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(&[order_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].quantity, Quantity::from_str("3.000000").unwrap());
        assert_eq!(
            items[0].cost_snapshot_total_gross,
            Amount::from_str("29.97").unwrap()
        );
        assert_eq!(items[0].input_tax_rate, Rate::from_str("0.130000").unwrap());
        assert_eq!(items[0].mall_order_item_id, item.mall_order_item_id);

        let action_found = db
            .supplier_order_actions()
            .find_by_idempotency_key("FO-1", &mut NoTransaction)
            .await
            .unwrap()
            .expect("PLACE 动作应按幂等键可读回");
        assert_eq!(action_found.action_type, SupplierOrderActionType::Place);
        assert_eq!(action_found.attempt_count, 0);
    })
}

#[tokio::test]
#[ignore]
async fn update_optimistic_lock_success_and_stale_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_optlock").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order = sample_order("FO-2");
        db.supplier_fulfillment_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(order.base.version, 1);

        order
            .update(SupplierFulfillmentOrderUpdate {
                external_order_no: Some("SUP-1001".to_string()),
            })
            .unwrap();
        db.supplier_fulfillment_orders()
            .update(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(order.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(order.external_order_no.as_deref(), Some("SUP-1001"));

        let mut stale = order.clone();
        order
            .update(SupplierFulfillmentOrderUpdate {
                external_order_no: Some("SUP-1002".to_string()),
            })
            .unwrap();
        db.supplier_fulfillment_orders()
            .update(&mut order, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(SupplierFulfillmentOrderUpdate {
                external_order_no: Some("SUP-1003".to_string()),
            })
            .unwrap();
        let error = db
            .supplier_fulfillment_orders()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 2, "CAS 失败不得改动内存版本");
    })
}

#[tokio::test]
#[ignore]
async fn soft_delete_and_restore_fulfillment_order() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_softdel").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order = sample_order("FO-3");
        db.supplier_fulfillment_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();

        db.supplier_fulfillment_orders()
            .soft_delete(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .supplier_fulfillment_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.supplier_fulfillment_orders()
            .restore(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .supplier_fulfillment_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_identities_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("FO-4");
        db.supplier_fulfillment_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate_order = sample_order("FO-4");
        let order_error = db
            .supplier_fulfillment_orders()
            .create(&duplicate_order, &mut NoTransaction)
            .await
            .expect_err("重复 fulfillment_order_no 必须被唯一索引拒绝");
        assert!(
            matches!(order_error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {order_error:?}"
        );

        let action = sample_place_action(&order.base.id.clone().into(), "FO-4");
        db.supplier_order_actions()
            .create(&action, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_action = sample_place_action(&order.base.id.clone().into(), "FO-4");
        let action_error = db
            .supplier_order_actions()
            .create(&duplicate_action, &mut NoTransaction)
            .await
            .expect_err("重复 idempotency_key 必须被唯一索引拒绝");
        assert!(
            matches!(action_error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {action_error:?}"
        );

        let item = sample_item(&order.base.id.clone().into(), "4");
        db.supplier_fulfillment_items()
            .create(&item, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_item = sample_item(&order.base.id.clone().into(), "4");
        let item_error = db
            .supplier_fulfillment_items()
            .create(&duplicate_item, &mut NoTransaction)
            .await
            .expect_err("重复 mall_order_item_id 必须被唯一索引拒绝");
        assert!(
            matches!(item_error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {item_error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn refund_fact_identity_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_refund_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("FO-5");
        db.supplier_fulfillment_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let order_id = order.base.id.clone().into();
        let (fact, allocation) = sample_refund(&order_id, "REF-1");

        let db_clone = db.clone();
        let fact_for_tx = fact.clone();
        let allocation_for_tx = allocation.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_fulfillment()
                        .create_refund_fact_with_allocations(&fact_for_tx, &[allocation_for_tx], session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("退款事实事务提交应成功");

        let found = db
            .supplier_refund_facts()
            .find_by_connection_and_refund(
                &SupplierApiConnectionId::new("connection-1"),
                "REF-1",
                "1",
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("按外部退款身份应可读回");
        assert_eq!(found.refund_amount, Amount::from_str("19.98").unwrap());

        let (duplicate_fact, duplicate_allocation) = sample_refund(&order_id, "REF-1");
        let db_clone = db.clone();
        let duplicate_fact_for_tx = duplicate_fact.clone();
        let duplicate_allocation_for_tx = duplicate_allocation.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_fulfillment()
                        .create_refund_fact_with_allocations(
                            &duplicate_fact_for_tx,
                            &[duplicate_allocation_for_tx],
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await;
        let error = result.expect_err("重复外部退款身份必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn list_search_respects_filters_pagination_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut accepted = sample_order("FO-6");
        accepted
            .advance_fulfillment(FulfillmentStatus::Submitting)
            .unwrap();
        accepted.advance_fulfillment(FulfillmentStatus::Accepted).unwrap();
        let mut shipped = sample_order("FO-7");
        shipped
            .advance_fulfillment(FulfillmentStatus::Submitting)
            .unwrap();
        shipped.advance_fulfillment(FulfillmentStatus::Accepted).unwrap();
        shipped
            .advance_fulfillment(FulfillmentStatus::Fulfilling)
            .unwrap();
        shipped.advance_fulfillment(FulfillmentStatus::Shipped).unwrap();
        let received_other_supplier = sample_order("FO-8");
        db.supplier_fulfillment_orders()
            .create(&accepted, &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_fulfillment_orders()
            .create(&shipped, &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_fulfillment_orders()
            .create(&received_other_supplier, &mut NoTransaction)
            .await
            .unwrap();

        let filter = SupplierFulfillmentOrderFilter {
            supplier_id: Some(SupplierAccountId::new("supplier-FO-6")),
            fulfillment_status: None,
            external_order_no: None,
            mall_order_id: None,
            page: 1,
            page_size: 1,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_fulfillment_orders()
            .search_supplier_fulfillment_orders(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "supplier-FO-6 只有一条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.fulfillment_order_no, "FO-6");
        assert_eq!(row.supplier_id, SupplierAccountId::new("supplier-FO-6"));
        assert_eq!(row.fulfillment_status, FulfillmentStatus::Accepted);
        assert_eq!(row.cancel_status, CancelStatus::None);
        assert!(row.submitted_at.is_some());
        assert!(row.accepted_at.is_some());
        assert!(row.version >= 1);
        assert!(row.created_at > 0);
        assert!(row.external_order_no.is_none());

        let boundary = SupplierFulfillmentOrderFilter {
            supplier_id: None,
            fulfillment_status: None,
            external_order_no: None,
            mall_order_id: None,
            page: 2,
            page_size: 2,
            sort_by: None,
            sort_ascending: false,
        };
        let boundary_page = db
            .supplier_fulfillment_orders()
            .search_supplier_fulfillment_orders(&boundary, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(boundary_page.total, 3);
        assert_eq!(boundary_page.items.len(), 1, "第二页（每页 2 条）只剩 1 条");

        let whitelist_sort = SupplierFulfillmentOrderFilter {
            supplier_id: None,
            fulfillment_status: None,
            external_order_no: None,
            mall_order_id: None,
            page: 1,
            page_size: 20,
            sort_by: Some("fulfillment_status".to_string()),
            sort_ascending: false,
        };
        let whitelist_page = db
            .supplier_fulfillment_orders()
            .search_supplier_fulfillment_orders(&whitelist_sort, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(
            whitelist_page.total, 3,
            "白名单外的排序字段必须回退默认排序而不是报错"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_creation_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("FO-10");
        let order_id = order.base.id.clone().into();
        let item = sample_item(&order_id, "10");
        let action = sample_place_action(&order_id, "FO-10");

        let db_clone = db.clone();
        let order_for_tx = order.clone();
        let item_for_tx = item.clone();
        let action_for_tx = action.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_fulfillment()
                        .create_fulfillment_with_items_and_place_action(
                            &order_for_tx,
                            &[item_for_tx],
                            &action_for_tx,
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let order_found = db
            .supplier_fulfillment_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(order_found.is_some(), "事务提交后订单必须可见");
        let items = db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(std::slice::from_ref(&order_id), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(items.len(), 1, "事务提交后明细必须可见");
        let action_found = db
            .supplier_order_actions()
            .find_by_idempotency_key("FO-10", &mut NoTransaction)
            .await
            .unwrap();
        assert!(action_found.is_some(), "事务提交后动作必须可见");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_creation_conflict_rolls_back_whole_creation() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("FO-11");
        let order_id = order.base.id.clone().into();
        let item = sample_item(&order_id, "11");
        let action = sample_place_action(&order_id, "FO-11");
        let db_clone = db.clone();
        let order_for_tx = order.clone();
        let item_for_tx = item.clone();
        let action_for_tx = action.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_fulfillment()
                        .create_fulfillment_with_items_and_place_action(
                            &order_for_tx,
                            &[item_for_tx],
                            &action_for_tx,
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("首批写入应成功");
        let mall_order_item_id = item.mall_order_item_id.clone();

        let conflicting = sample_order("FO-12");
        let conflicting_id: SupplierFulfillmentOrderId = conflicting.base.id.clone().into();
        let conflicting_item = SupplierFulfillmentItem::new(
            SupplierFulfillmentItemId::new("item-12"),
            SupplierFulfillmentItemData {
                supplier_fulfillment_order_id: conflicting_id.clone(),
                mall_order_item_id: mall_order_item_id.clone(),
                supplier_offering_revision_id: SupplierOfferingRevisionId::new("offering-12"),
                supplier_catalog_sku_id: SupplierCatalogSkuId::new("sku-12"),
                quantity: Quantity::from_str("3.000000").unwrap(),
                unit_cost_snapshot_gross: UnitPrice::from_str("9.9900").unwrap(),
                cost_snapshot_total_gross: Amount::from_str("29.97").unwrap(),
                input_tax_rate: Rate::from_str("0.130000").unwrap(),
            },
        )
        .unwrap();
        let conflicting_action = sample_place_action(&conflicting_id, "FO-12");

        let db_clone = db.clone();
        let conflicting_for_tx = conflicting.clone();
        let conflicting_item_for_tx = conflicting_item.clone();
        let conflicting_action_for_tx = conflicting_action.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_fulfillment()
                        .create_fulfillment_with_items_and_place_action(
                            &conflicting_for_tx,
                            &[conflicting_item_for_tx],
                            &conflicting_action_for_tx,
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await;
        let error = result.expect_err("明细 mall_order_item_id 冲突必须使事务失败");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let order_after = db
            .supplier_fulfillment_orders()
            .find_by_id(&conflicting.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(order_after.is_none(), "回滚后订单不得残留");
        let items_after = db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(&[conflicting_id], &mut NoTransaction)
            .await
            .unwrap();
        assert!(items_after.is_empty(), "回滚后明细不得残留");
        let action_after = db
            .supplier_order_actions()
            .find_by_idempotency_key("FO-12", &mut NoTransaction)
            .await
            .unwrap();
        assert!(action_after.is_none(), "回滚后动作不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn no_transaction_multi_step_writes_are_independently_committed() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("FO-13");
        let order_id = order.base.id.clone().into();
        let item = sample_item(&order_id, "13");
        let action = sample_place_action(&order_id, "FO-13");

        db.supplier_fulfillment()
            .create_fulfillment_with_items_and_place_action(&order, &[item], &action, &mut NoTransaction)
            .await
            .unwrap();

        let order_found = db
            .supplier_fulfillment_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(order_found.is_some(), "NoTransaction 下订单按自动提交语义写入");
        let items = db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(&[order_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(items.len(), 1, "NoTransaction 下明细按自动提交语义写入");
        let action_found = db
            .supplier_order_actions()
            .find_by_idempotency_key("FO-13", &mut NoTransaction)
            .await
            .unwrap();
        assert!(action_found.is_some(), "NoTransaction 下动作按自动提交语义写入");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_order_and_status_history() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_ff_tx_hist").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("FO-14");
        let history = sample_status_history(&SupplierApiConnectionId::new("connection-1"), "EVT-14");

        let db_clone = db.clone();
        let order_for_tx = order.clone();
        let history_for_tx = history.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier_fulfillment_orders()
                        .create(&order_for_tx, session)
                        .await?;
                    db_clone
                        .supplier_order_status_histories()
                        .create(&history_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let order_found = db
            .supplier_fulfillment_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(order_found.is_none(), "回滚后订单不得残留");
        let history_found = db
            .supplier_order_status_histories()
            .find_by_connection_and_event(
                &SupplierApiConnectionId::new("connection-1"),
                "EVT-14",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(history_found.is_none(), "回滚后状态历史不得残留");
    })
}
