//! 域 D16 `fulfillment` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test fulfillment_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。
//!
//! 覆盖：创建+读取往返（含 Decimal128 数量与时间字段）、乐观锁成功/冲突、
//! 软删除与恢复（仅草稿单据）、唯一索引冲突、索引存在性、事务参与（回滚后
//! 跨集合均不可见）、列表查询（分页边界/排序白名单/投影字段集）、多步骤
//! 方法（事务内冲突整体回滚）。

use database::repository::extensions::FulfillmentExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::fulfillment::{
    AcceptanceFulfillmentAllocation, AcceptanceFulfillmentAllocationData, AcceptanceResult, AllocationAction,
    CustomerAcceptance, CustomerAcceptanceData, CustomerAcceptanceLine, CustomerAcceptanceLineData,
    CustomerAcceptanceState, Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryState,
    DeliveryType, ElectronicDelivery, ElectronicDeliveryData, FulfillmentFactType, FulfillmentResult,
    PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine, PurchaseReceiptLineData, PurchaseReceiptState,
    QualityResult, ServiceFulfillment, ServiceFulfillmentData,
};
use entities::ids::{
    AcceptanceFulfillmentAllocationId, CustomerAcceptanceId, CustomerAcceptanceLineId, DeliveryId,
    DeliveryLineId, ElectronicDeliveryId, PurchaseLineSalesAllocationId, PurchaseOrderId,
    PurchaseOrderRevisionLineId, PurchaseReceiptId, PurchaseReceiptLineId, SalesOrderId, SalesOrderLineId,
    ServiceFulfillmentId, StockReservationId, WarehouseId,
};
use entities::money::Quantity;
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 采购入库单列表筛选条件类型（经 `FulfillmentExt` 关联类型跨 crate 可达）。
type PurchaseReceiptFilter = <Database as FulfillmentExt>::PurchaseReceiptFilter;
/// 发货单列表筛选条件类型。
type DeliveryFilter = <Database as FulfillmentExt>::DeliveryFilter;

/// 构造采购入库单实体（草稿）。
fn sample_receipt(id: &str, receipt_no: &str) -> PurchaseReceipt {
    PurchaseReceipt::new(
        PurchaseReceiptId::new(id),
        PurchaseReceiptData {
            receipt_no: receipt_no.to_string(),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            warehouse_id: WarehouseId::new("wh-1"),
        },
    )
    .unwrap()
}

/// 构造采购入库行实体（到货 10、合格 9、不合格 1）。
fn sample_receipt_line(id: &str, receipt_id: &str, line_no: u32) -> PurchaseReceiptLine {
    PurchaseReceiptLine::new(
        PurchaseReceiptLineId::new(id),
        PurchaseReceiptLineData {
            purchase_receipt_id: PurchaseReceiptId::new(receipt_id),
            line_no,
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("po-line-1"),
            received_quantity: Quantity::from_str("10").unwrap(),
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            quality_result: QualityResult::Partial,
        },
    )
    .unwrap()
}

/// 构造仓发发货单实体（草稿）。
fn sample_delivery(id: &str, delivery_no: &str) -> Delivery {
    Delivery::new(
        DeliveryId::new(id),
        DeliveryData {
            delivery_no: delivery_no.to_string(),
            delivery_type: DeliveryType::WarehouseShip,
            sales_order_id: SalesOrderId::new("so-1"),
            purchase_order_id: None,
            warehouse_id: Some(WarehouseId::new("wh-1")),
            carrier: Some("顺丰".to_string()),
            tracking_no: Some("SF-001".to_string()),
            address_snapshot_encrypted: None,
            address_snapshot_fingerprint: None,
        },
    )
    .unwrap()
}

/// 构造仓发发货行实体。
fn sample_delivery_line(id: &str, delivery_id: &str, line_no: u32) -> DeliveryLine {
    DeliveryLine::new(
        DeliveryLineId::new(id),
        DeliveryLineData {
            delivery_id: DeliveryId::new(delivery_id),
            line_no,
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            quantity: Quantity::from_str("3").unwrap(),
            stock_reservation_id: Some(StockReservationId::new("rsv-1")),
            purchase_line_sales_allocation_id: None,
        },
        DeliveryType::WarehouseShip,
    )
    .unwrap()
}

/// 构造电子交付记录实体（草稿）。
fn sample_electronic_delivery(id: &str, fulfillment_no: &str) -> ElectronicDelivery {
    ElectronicDelivery::new(
        ElectronicDeliveryId::new(id),
        ElectronicDeliveryData {
            fulfillment_no: fulfillment_no.to_string(),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
            recipient_snapshot: "ciphertext-recipient".to_string(),
            recipient_snapshot_fingerprint: ElectronicDelivery::recipient_snapshot_fingerprint(
                "收件人 李四",
                b"test-key",
            ),
            quantity: Quantity::from_str("2").unwrap(),
            result: FulfillmentResult::Success,
            evidence_attachment_id: None,
            fact_no: "F-ED-1".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
            recorded_by: "operator-1".to_string(),
            source_type: entities::common::source::SourceType::Erp,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )
    .unwrap()
}

/// 构造线下服务履约记录实体（草稿）。
fn sample_service_fulfillment(id: &str, fulfillment_no: &str) -> ServiceFulfillment {
    ServiceFulfillment::new(
        ServiceFulfillmentId::new(id),
        ServiceFulfillmentData {
            fulfillment_no: fulfillment_no.to_string(),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
            recipient_snapshot: "ciphertext-recipient".to_string(),
            recipient_snapshot_fingerprint: ServiceFulfillment::recipient_snapshot_fingerprint(
                "收件人 王五",
                b"test-key",
            ),
            quantity: Quantity::from_str("1").unwrap(),
            result: FulfillmentResult::Success,
            evidence_attachment_id: None,
            service_location_encrypted: "ciphertext-location".to_string(),
            service_location_fingerprint: ServiceFulfillment::service_location_fingerprint(
                "上海市徐汇区",
                b"test-key",
            ),
            service_started_at: None,
            service_ended_at: None,
            completion_note: None,
            fact_no: "F-SF-1".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
            recorded_by: "operator-1".to_string(),
            source_type: entities::common::source::SourceType::ManualImport,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )
    .unwrap()
}

/// 构造客户验收单实体（草稿）。
fn sample_acceptance(id: &str, acceptance_no: &str) -> CustomerAcceptance {
    CustomerAcceptance::new(
        CustomerAcceptanceId::new(id),
        CustomerAcceptanceData {
            acceptance_no: acceptance_no.to_string(),
            sales_order_id: SalesOrderId::new("so-1"),
            accepted_at: Instant::from_unix_secs(1_700_000_000),
            result: AcceptanceResult::Passed,
        },
    )
    .unwrap()
}

/// 构造客户验收行实体。
fn sample_acceptance_line(id: &str, acceptance_id: &str, line_no: u32) -> CustomerAcceptanceLine {
    CustomerAcceptanceLine::new(
        CustomerAcceptanceLineId::new(id),
        CustomerAcceptanceLineData {
            customer_acceptance_id: CustomerAcceptanceId::new(acceptance_id),
            line_no,
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            accepted_quantity: Quantity::from_str("9").unwrap(),
            short_quantity: Quantity::from_str("1").unwrap(),
            rejected_quantity: Quantity::from_str("0").unwrap(),
            reason: None,
            evidence_attachment_id: None,
        },
    )
    .unwrap()
}

/// 构造验收履约分配实体（APPLY）。
fn sample_allocation(id: &str, acceptance_line_id: &str) -> AcceptanceFulfillmentAllocation {
    AcceptanceFulfillmentAllocation::new(
        AcceptanceFulfillmentAllocationId::new(id),
        AcceptanceFulfillmentAllocationData {
            customer_acceptance_line_id: CustomerAcceptanceLineId::new(acceptance_line_id),
            fulfillment_fact_type: FulfillmentFactType::Delivery,
            fulfillment_line_id: "dl-1".to_string(),
            allocation_action: AllocationAction::Apply,
            allocated_quantity: Quantity::from_str("2").unwrap(),
            reverses_allocation_id: None,
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as FulfillmentExt>::PURCHASE_RECEIPTS,
        &[
            "uk_purchase_receipts_receipt_no",
            "idx_purchase_receipts_po_status_posted",
        ],
    )
    .await
    .expect("purchase_receipts 索引缺失");
    assert_indexes(
        db,
        <Database as FulfillmentExt>::PURCHASE_RECEIPT_LINES,
        &["uk_purchase_receipt_lines_header_line"],
    )
    .await
    .expect("purchase_receipt_lines 索引缺失");
    assert_indexes(
        db,
        <Database as FulfillmentExt>::DELIVERIES,
        &[
            "uk_deliveries_delivery_no",
            "idx_deliveries_sales_order_status",
            "idx_deliveries_tracking_no",
        ],
    )
    .await
    .expect("deliveries 索引缺失");
    assert_indexes(
        db,
        <Database as FulfillmentExt>::DELIVERY_LINES,
        &["uk_delivery_lines_header_line"],
    )
    .await
    .expect("delivery_lines 索引缺失");
    assert_indexes(
        db,
        <Database as FulfillmentExt>::ELECTRONIC_DELIVERIES,
        &[
            "uk_electronic_deliveries_fulfillment_no",
            "idx_electronic_deliveries_line_occurred",
        ],
    )
    .await
    .expect("electronic_deliveries 索引缺失");
    assert_indexes(
        db,
        <Database as FulfillmentExt>::SERVICE_FULFILLMENTS,
        &[
            "uk_service_fulfillments_fulfillment_no",
            "idx_service_fulfillments_line_occurred",
        ],
    )
    .await
    .expect("service_fulfillments 索引缺失");
    assert_indexes(
        db,
        <Database as FulfillmentExt>::CUSTOMER_ACCEPTANCES,
        &[
            "uk_customer_acceptances_acceptance_no",
            "idx_customer_acceptances_sales_order_accepted",
        ],
    )
    .await
    .expect("customer_acceptances 索引缺失");
    assert_indexes(
        db,
        <Database as FulfillmentExt>::CUSTOMER_ACCEPTANCE_LINES,
        &["uk_customer_acceptance_lines_header_line"],
    )
    .await
    .expect("customer_acceptance_lines 索引缺失");
    assert_indexes(
        db,
        <Database as FulfillmentExt>::ACCEPTANCE_FULFILLMENT_ALLOCATIONS,
        &[
            "idx_acceptance_fulfillment_allocations_acceptance_line",
            "idx_acceptance_fulfillment_allocations_fulfillment_fact",
        ],
    )
    .await
    .expect("acceptance_fulfillment_allocations 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_read_roundtrip_covers_decimal_and_time_fields() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_fulfill_rw").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let receipt = sample_receipt("receipt-1", "PR-2026-001");
        let lines = vec![
            sample_receipt_line("rl-1", "receipt-1", 1),
            sample_receipt_line("rl-2", "receipt-1", 2),
        ];
        db.fulfillment()
            .create_purchase_receipt_with_lines(&receipt, &lines, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(receipt.base.version, 1);

        let found = db
            .purchase_receipts()
            .find_by_id(&receipt.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.receipt_no, "PR-2026-001");
        assert_eq!(found.status, PurchaseReceiptState::Draft);
        assert_eq!(found.warehouse_id, WarehouseId::new("wh-1"));

        let loaded_lines = db
            .fulfillment()
            .receipt_lines_by_receipt_ids(&[PurchaseReceiptId::new("receipt-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(loaded_lines.len(), 2);
        assert_eq!(loaded_lines[0].line_no, 1);
        assert_eq!(
            loaded_lines[0].qualified_quantity,
            Quantity::from_str("9").unwrap(),
            "Decimal128 数量往返一致"
        );

        let mut delivery = sample_delivery("delivery-1", "DV-2026-001");
        delivery
            .mark_shipped(Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        db.fulfillment()
            .create_delivery_with_lines(
                &delivery,
                &[sample_delivery_line("dl-1", "delivery-1", 1)],
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let shipped = db
            .deliveries()
            .find_by_id(&delivery.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("发货单应可读回");
        assert_eq!(shipped.status, DeliveryState::Shipped);
        assert_eq!(
            shipped.shipped_at.unwrap().unix_secs(),
            1_700_000_000,
            "时间字段往返一致"
        );

        let electronic = sample_electronic_delivery("ed-1", "ED-2026-001");
        db.electronic_deliveries()
            .create(&electronic, &mut NoTransaction)
            .await
            .unwrap();
        let service = sample_service_fulfillment("sf-1", "SF-2026-001");
        db.service_fulfillments()
            .create(&service, &mut NoTransaction)
            .await
            .unwrap();
        assert!(db
            .electronic_deliveries()
            .find_by_id("ed-1", &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(db
            .service_fulfillments()
            .find_by_id("sf-1", &mut NoTransaction)
            .await
            .unwrap()
            .is_some());

        let acceptance = sample_acceptance("acceptance-1", "CA-2026-001");
        db.fulfillment()
            .create_customer_acceptance_with_lines(
                &acceptance,
                &[sample_acceptance_line("cal-1", "acceptance-1", 1)],
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let allocation = sample_allocation("allocation-1", "cal-1");
        db.acceptance_fulfillment_allocations()
            .create(&allocation, &mut NoTransaction)
            .await
            .unwrap();
        let accepted = db
            .customer_acceptances()
            .find_by_id(&acceptance.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("验收单应可读回");
        assert_eq!(accepted.status, CustomerAcceptanceState::Draft);
        assert_eq!(accepted.accepted_at.unix_secs(), 1_700_000_000);
        let allocations = db
            .fulfillment()
            .allocations_by_acceptance_lines(&[CustomerAcceptanceLineId::new("cal-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(allocations.len(), 1);
        assert_eq!(allocations[0].fulfillment_line_id, "dl-1");
        let fact_allocations = db
            .fulfillment()
            .allocations_by_fulfillment_fact(
                FulfillmentFactType::Delivery,
                &["dl-1".to_string()],
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(fact_allocations.len(), 1);

        assert!(db
            .deliveries()
            .find_by_tracking_no("SF-001", &mut NoTransaction)
            .await
            .unwrap()
            .iter()
            .any(|item| item.delivery_no == "DV-2026-001"));
    })
}

#[tokio::test]
#[ignore]
async fn optimistic_lock_success_and_stale_version_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_fulfill_opt").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut receipt = sample_receipt("receipt-opt", "PR-OPT-001");
        db.purchase_receipts()
            .create(&receipt, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(receipt.base.version, 1);

        receipt
            .mark_posted(Instant::from_unix_secs(1_700_000_000), "operator-1")
            .unwrap();
        db.purchase_receipts()
            .update(&mut receipt, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(receipt.base.version, 2, "乐观锁成功后 version 递增");

        let mut stale = sample_receipt("receipt-opt-stale", "PR-OPT-002");
        db.purchase_receipts()
            .create(&stale, &mut NoTransaction)
            .await
            .unwrap();
        let mut moved = stale.clone();
        moved.warehouse_id = WarehouseId::new("wh-2");
        db.purchase_receipts()
            .update(&mut moved, &mut NoTransaction)
            .await
            .unwrap();
        let error = db
            .purchase_receipts()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 1, "CAS 失败不得改动内存版本");
    })
}

#[tokio::test]
#[ignore]
async fn draft_soft_delete_and_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_fulfill_del").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut receipt = sample_receipt("receipt-del", "PR-DEL-001");
        db.purchase_receipts()
            .create(&receipt, &mut NoTransaction)
            .await
            .unwrap();
        db.purchase_receipts()
            .soft_delete(&mut receipt, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            db.purchase_receipts()
                .find_by_id(&receipt.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "软删除后按 ID 不可见"
        );
        assert!(db
            .purchase_receipts()
            .find_by_receipt_no("PR-DEL-001", &mut NoTransaction)
            .await
            .unwrap()
            .is_none());

        db.purchase_receipts()
            .restore(&mut receipt, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            db.purchase_receipts()
                .find_by_id(&receipt.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "恢复后按 ID 重新可见"
        );
    })
}

#[tokio::test]
#[ignore]
async fn unique_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_fulfill_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let receipt = sample_receipt("receipt-dup", "PR-DUP-001");
        db.purchase_receipts()
            .create(&receipt, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_receipt = sample_receipt("receipt-dup-2", "PR-DUP-001");
        let error = db
            .purchase_receipts()
            .create(&duplicate_receipt, &mut NoTransaction)
            .await
            .expect_err("重复 receipt_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let line = sample_receipt_line("rl-dup", "receipt-dup", 1);
        db.purchase_receipt_lines()
            .create(&line, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_line = sample_receipt_line("rl-dup-2", "receipt-dup", 1);
        let error = db
            .purchase_receipt_lines()
            .create(&duplicate_line, &mut NoTransaction)
            .await
            .expect_err("重复 (receipt, line_no) 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let delivery = sample_delivery("delivery-dup", "DV-DUP-001");
        db.deliveries()
            .create(&delivery, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_delivery = sample_delivery("delivery-dup-2", "DV-DUP-001");
        let error = db
            .deliveries()
            .create(&duplicate_delivery, &mut NoTransaction)
            .await
            .expect_err("重复 delivery_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let acceptance = sample_acceptance("acceptance-dup", "CA-DUP-001");
        db.customer_acceptances()
            .create(&acceptance, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_acceptance = sample_acceptance("acceptance-dup-2", "CA-DUP-001");
        let error = db
            .customer_acceptances()
            .create(&duplicate_acceptance, &mut NoTransaction)
            .await
            .expect_err("重复 acceptance_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let electronic = sample_electronic_delivery("ed-dup", "ED-DUP-001");
        db.electronic_deliveries()
            .create(&electronic, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_electronic = sample_electronic_delivery("ed-dup-2", "ED-DUP-001");
        let error = db
            .electronic_deliveries()
            .create(&duplicate_electronic, &mut NoTransaction)
            .await
            .expect_err("重复 fulfillment_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn receipt_list_search_respects_pagination_sort_whitelist_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_fulfill_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        for (index, receipt_no) in ["PR-LIST-001", "PR-LIST-002", "PR-LIST-003"].iter().enumerate() {
            let mut receipt = sample_receipt(&format!("receipt-list-{index}"), receipt_no);
            receipt
                .mark_posted(
                    Instant::from_unix_secs(1_700_000_000 + index as i64),
                    "operator-1",
                )
                .unwrap();
            db.purchase_receipts()
                .create(&receipt, &mut NoTransaction)
                .await
                .unwrap();
        }

        let filter = PurchaseReceiptFilter {
            purchase_order_id: Some(PurchaseOrderId::new("po-1")),
            status: Some(PurchaseReceiptState::Posted),
            page: 1,
            page_size: 2,
            sort_by: Some("posted_at".to_string()),
            sort_ascending: true,
        };
        let first = db
            .purchase_receipts()
            .search_purchase_receipts(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(first.total, 3, "总数按筛选条件统计");
        assert_eq!(first.items.len(), 2, "分页边界：第一页两行");
        assert_eq!(
            first.items[0].receipt_no, "PR-LIST-001",
            "排序白名单 posted_at 升序"
        );
        let row = &first.items[0];
        assert_eq!(row.purchase_order_id, PurchaseOrderId::new("po-1"));
        assert_eq!(row.status, PurchaseReceiptState::Posted);
        assert_eq!(row.posted_at.unwrap().unix_secs(), 1_700_000_000);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let second_page = PurchaseReceiptFilter {
            page: 2,
            page_size: 2,
            purchase_order_id: Some(PurchaseOrderId::new("po-1")),
            status: Some(PurchaseReceiptState::Posted),
            sort_by: Some("posted_at".to_string()),
            sort_ascending: true,
        };
        let second = db
            .purchase_receipts()
            .search_purchase_receipts(&second_page, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(second.items.len(), 1, "分页边界：第二页剩一行");
        assert_eq!(second.items[0].receipt_no, "PR-LIST-003");

        let off_whitelist = PurchaseReceiptFilter {
            sort_by: Some("任意字段".to_string()),
            ..filter
        };
        let fallback = db
            .purchase_receipts()
            .search_purchase_receipts(&off_whitelist, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(fallback.items.len(), 2, "白名单外排序字段回落默认排序");
    })
}

#[tokio::test]
#[ignore]
async fn delivery_list_search_filters_by_sales_order_and_status() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_fulfill_dv_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut shipped = sample_delivery("delivery-list-1", "DV-LIST-001");
        shipped
            .mark_shipped(Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        let mut draft = sample_delivery("delivery-list-2", "DV-LIST-002");
        draft.tracking_no = Some("SF-OTHER".to_string());
        db.deliveries()
            .create(&shipped, &mut NoTransaction)
            .await
            .unwrap();
        db.deliveries().create(&draft, &mut NoTransaction).await.unwrap();

        let filter = DeliveryFilter {
            sales_order_id: Some(SalesOrderId::new("so-1")),
            status: Some(DeliveryState::Shipped),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .deliveries()
            .search_deliveries(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.items[0];
        assert_eq!(row.delivery_no, "DV-LIST-001");
        assert_eq!(row.delivery_type, DeliveryType::WarehouseShip);
        assert_eq!(row.status, DeliveryState::Shipped);
        assert_eq!(row.tracking_no.as_deref(), Some("SF-001"));

        let by_tracking = db
            .deliveries()
            .find_by_tracking_no("SF-OTHER", &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_tracking.len(), 1);
        assert_eq!(by_tracking[0].delivery_no, "DV-LIST-002");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_commit_makes_header_and_lines_atomically_visible() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_fulfill_tx_ok").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let receipt = sample_receipt("receipt-tx-ok", "PR-TX-001");
        let lines = vec![sample_receipt_line("rl-tx-ok", "receipt-tx-ok", 1)];
        let db_clone = db.clone();
        let receipt_for_tx = receipt.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .fulfillment()
                        .create_purchase_receipt_with_lines(&receipt_for_tx, &lines, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        assert!(db
            .purchase_receipts()
            .find_by_id(&receipt.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(db
            .purchase_receipt_lines()
            .find_by_id("rl-tx-ok", &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_header_and_lines_together() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_fulfill_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let receipt = sample_receipt("receipt-tx-abort", "PR-TX-002");
        let lines = vec![sample_receipt_line("rl-tx-abort", "receipt-tx-abort", 1)];
        let db_clone = db.clone();
        let receipt_for_tx = receipt.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .fulfillment()
                        .create_purchase_receipt_with_lines(&receipt_for_tx, &lines, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        assert!(
            db.purchase_receipts()
                .find_by_id(&receipt.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后表头不得残留"
        );
        assert!(
            db.purchase_receipt_lines()
                .find_by_id("rl-tx-abort", &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后行不得残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_with_no_transaction_writes_are_predictable() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_fulfill_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let receipt = sample_receipt("receipt-notx", "PR-NOTX-001");
        let lines = vec![sample_receipt_line("rl-notx", "receipt-notx", 1)];
        db.fulfillment()
            .create_purchase_receipt_with_lines(&receipt, &lines, &mut NoTransaction)
            .await
            .unwrap();

        assert!(db
            .purchase_receipts()
            .find_by_id(&receipt.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(db
            .purchase_receipt_lines()
            .find_by_id("rl-notx", &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
    })
}
