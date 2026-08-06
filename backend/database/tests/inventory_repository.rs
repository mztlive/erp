//! 域 D17 `inventory` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test inventory_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。
//!
//! 覆盖：创建+读取往返（含 Decimal128 数量与时间字段）、乐观锁成功/冲突、
//! 唯一索引冲突、索引存在性、事务参与（回滚后跨集合均不可见）、列表查询
//! （分页边界/排序白名单/投影字段集）、多步骤方法（事务内冲突整体回滚），
//! ★以及余额/预占原子条件写：可用量充足→扣减成功；可用量不足→整体拒绝
//! （写条件未命中，文档不变）。

use database::repository::extensions::InventoryExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::ids::{
    PurchaseLineSalesAllocationId, PurchaseReceiptLineId, SalesOrderLineId, SkuId, StockAdjustmentId,
    StockAdjustmentLineId, StockBalanceId, StockMovementId, StockReservationEntryId, StockReservationId,
    WarehouseId,
};
use entities::inventory::{
    AdjustmentReasonType, MovementDirection, MovementType, ReservationEntryType, ReservationStatus,
    StockAdjustment, StockAdjustmentData, StockAdjustmentLine, StockAdjustmentLineData, StockAdjustmentState,
    StockBalance, StockBalanceData, StockMovement, StockMovementData, StockReservation, StockReservationData,
    StockReservationEntry, StockReservationEntryData,
};
use entities::money::Quantity;
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 库存流水列表筛选条件类型（经 `InventoryExt` 关联类型跨 crate 可达）。
type StockMovementFilter = <Database as InventoryExt>::StockMovementFilter;
/// 库存余额列表筛选条件类型。
type StockBalanceFilter = <Database as InventoryExt>::StockBalanceFilter;
/// 库存预占列表筛选条件类型。
type StockReservationFilter = <Database as InventoryExt>::StockReservationFilter;
/// 库存调整单列表筛选条件类型。
type StockAdjustmentFilter = <Database as InventoryExt>::StockAdjustmentFilter;

/// 构造库存流水实体（采购入库，增加 10）。
fn sample_movement(id: &str, source_document_id: &str, occurred_secs: i64) -> StockMovement {
    StockMovement::new(
        StockMovementId::new(id),
        StockMovementData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            movement_type: MovementType::PurchaseReceiptIn,
            direction: MovementDirection::Increase,
            quantity: Quantity::from_str("10").unwrap(),
            source_document_id: source_document_id.to_string(),
            source_line_id: Some(format!("{source_document_id}-line-1")),
            reversal_of_movement_id: None,
            fact_no: format!("F-{id}"),
            occurred_at: Instant::from_unix_secs(occurred_secs),
            recorded_at: Instant::from_unix_secs(occurred_secs + 100),
            recorded_by: "operator-1".to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )
    .unwrap()
}

/// 构造库存余额实体（现存量 100、预占 30、可用 70）。
fn sample_balance(id: &str, sku_id: &str) -> StockBalance {
    StockBalance::new(
        StockBalanceId::new(id),
        StockBalanceData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new(sku_id),
            on_hand_quantity: Quantity::from_str("100").unwrap(),
            reserved_quantity: Quantity::from_str("30").unwrap(),
            available_quantity: Quantity::from_str("70").unwrap(),
            last_movement_id: None,
        },
    )
    .unwrap()
}

/// 构造库存预占实体（有效预占 10）。
fn sample_reservation(id: &str, reserved: &str, sku_id: &str) -> StockReservation {
    StockReservation::new(
        StockReservationId::new(id),
        StockReservationData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new(sku_id),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
            source_receipt_line_id: PurchaseReceiptLineId::new("receipt-line-1"),
            reserved_quantity: Quantity::from_str(reserved).unwrap(),
            consumed_quantity: Quantity::from_str("0").unwrap(),
            released_quantity: Quantity::from_str("0").unwrap(),
            status: ReservationStatus::Active,
        },
    )
    .unwrap()
}

/// 构造预占流水实体（建立 10）。
fn sample_reservation_entry(id: &str, reservation_id: &str) -> StockReservationEntry {
    StockReservationEntry::new(
        StockReservationEntryId::new(id),
        StockReservationEntryData {
            reservation_id: StockReservationId::new(reservation_id),
            entry_type: ReservationEntryType::Establish,
            quantity: Quantity::from_str("10").unwrap(),
            source_document_id: "receipt-line-1".to_string(),
        },
    )
    .unwrap()
}

/// 构造库存调整单实体（草稿，盘亏）。
fn sample_adjustment(id: &str, adjustment_no: &str) -> StockAdjustment {
    StockAdjustment::new(
        StockAdjustmentId::new(id),
        StockAdjustmentData {
            adjustment_no: adjustment_no.to_string(),
            warehouse_id: WarehouseId::new("wh-1"),
            reason_type: AdjustmentReasonType::StockLoss,
            prepared_by: "operator-1".to_string(),
        },
    )
    .unwrap()
}

/// 构造库存调整明细实体（减少 2）。
fn sample_adjustment_line(id: &str, adjustment_id: &str, sku_id: &str) -> StockAdjustmentLine {
    StockAdjustmentLine::new(
        StockAdjustmentLineId::new(id),
        StockAdjustmentLineData {
            stock_adjustment_id: StockAdjustmentId::new(adjustment_id),
            sku_id: SkuId::new(sku_id),
            quantity: Quantity::from_str("2").unwrap(),
            direction: MovementDirection::Decrease,
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as InventoryExt>::STOCK_MOVEMENTS,
        &["uk_stock_movements_source", "idx_stock_movements_ledger"],
    )
    .await
    .expect("stock_movements 索引缺失");
    assert_indexes(
        db,
        <Database as InventoryExt>::STOCK_BALANCES,
        &["uk_stock_balances_dimension"],
    )
    .await
    .expect("stock_balances 索引缺失");
    assert_indexes(
        db,
        <Database as InventoryExt>::STOCK_RESERVATIONS,
        &[
            "uk_stock_reservations_establish",
            "idx_stock_reservations_warehouse_sku_status",
            "idx_stock_reservations_sales_line_status",
        ],
    )
    .await
    .expect("stock_reservations 索引缺失");
    assert_indexes(
        db,
        <Database as InventoryExt>::STOCK_RESERVATION_ENTRIES,
        &["idx_stock_reservation_entries_reservation"],
    )
    .await
    .expect("stock_reservation_entries 索引缺失");
    assert_indexes(
        db,
        <Database as InventoryExt>::STOCK_ADJUSTMENTS,
        &[
            "uk_stock_adjustments_adjustment_no",
            "idx_stock_adjustments_warehouse_status",
        ],
    )
    .await
    .expect("stock_adjustments 索引缺失");
    assert_indexes(
        db,
        <Database as InventoryExt>::STOCK_ADJUSTMENT_LINES,
        &["idx_stock_adjustment_lines_adjustment"],
    )
    .await
    .expect("stock_adjustment_lines 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_read_roundtrip_covers_decimal_and_time_fields() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_rw").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let movement = sample_movement("m-1", "receipt-1", 1_700_000_000);
        db.stock_movements()
            .create(&movement, &mut NoTransaction)
            .await
            .unwrap();
        let found = db
            .stock_movements()
            .find_by_id(&movement.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.source_document_id, "receipt-1");
        assert_eq!(
            found.quantity,
            Quantity::from_str("10").unwrap(),
            "Decimal128 数量往返一致"
        );
        assert_eq!(
            found.fact.occurred_at.unix_secs(),
            1_700_000_000,
            "时间字段往返一致"
        );
        assert_eq!(found.direction, MovementDirection::Increase);

        let balance = sample_balance("b-1", "sku-1");
        db.stock_balances()
            .create(&balance, &mut NoTransaction)
            .await
            .unwrap();
        let by_dimensions = db
            .stock_balances()
            .find_by_dimensions(
                &WarehouseId::new("wh-1"),
                &SkuId::new("sku-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("按维度应可读回");
        assert_eq!(by_dimensions.on_hand_quantity, Quantity::from_str("100").unwrap());
        assert_eq!(
            by_dimensions.available_quantity,
            Quantity::from_str("70").unwrap()
        );

        let reservation = sample_reservation("rsv-1", "10", "sku-1");
        let entry = sample_reservation_entry("entry-1", "rsv-1");
        db.stock_reservations()
            .create(&reservation, &mut NoTransaction)
            .await
            .unwrap();
        db.stock_reservation_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();
        let entries = db
            .inventory()
            .reservation_entries_by_reservation_ids(&[StockReservationId::new("rsv-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_type, ReservationEntryType::Establish);

        let adjustment = sample_adjustment("adj-1", "ADJ-2026-001");
        let lines = vec![sample_adjustment_line("al-1", "adj-1", "sku-1")];
        db.inventory()
            .create_stock_adjustment_with_lines(&adjustment, &lines, &mut NoTransaction)
            .await
            .unwrap();
        let found_adjustment = db
            .stock_adjustments()
            .find_by_adjustment_no("ADJ-2026-001", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按单号应可读回");
        assert_eq!(found_adjustment.status, StockAdjustmentState::Draft);
        assert_eq!(found_adjustment.prepared_by, "operator-1");
        let loaded_lines = db
            .inventory()
            .adjustment_lines_by_adjustment_ids(&[StockAdjustmentId::new("adj-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(loaded_lines.len(), 1);
        assert_eq!(loaded_lines[0].quantity, Quantity::from_str("2").unwrap());

        let by_source = db
            .stock_movements()
            .find_by_source_document("receipt-1", &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_source.len(), 1);
        let by_ids = db
            .inventory()
            .movements_by_ids(&[StockMovementId::new("m-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_ids.len(), 1);
    })
}

#[tokio::test]
#[ignore]
async fn optimistic_lock_success_and_stale_version_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_opt").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut adjustment = sample_adjustment("adj-opt", "ADJ-OPT-001");
        db.stock_adjustments()
            .create(&adjustment, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(adjustment.base.version, 1);

        adjustment.submit_for_warehouse_review("reviewer-1").unwrap();
        db.stock_adjustments()
            .update(&mut adjustment, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(adjustment.base.version, 2, "乐观锁成功后 version 递增");

        let mut stale = sample_adjustment("adj-opt-stale", "ADJ-OPT-002");
        db.stock_adjustments()
            .create(&stale, &mut NoTransaction)
            .await
            .unwrap();
        let mut moved = stale.clone();
        moved.warehouse_id = WarehouseId::new("wh-2");
        db.stock_adjustments()
            .update(&mut moved, &mut NoTransaction)
            .await
            .unwrap();
        let error = db
            .stock_adjustments()
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
async fn unique_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let balance = sample_balance("b-dup", "sku-1");
        db.stock_balances()
            .create(&balance, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_balance = sample_balance("b-dup-2", "sku-1");
        let error = db
            .stock_balances()
            .create(&duplicate_balance, &mut NoTransaction)
            .await
            .expect_err("重复 (warehouse, sku) 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let movement = sample_movement("m-dup", "receipt-dup", 1_700_000_000);
        db.stock_movements()
            .create(&movement, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_movement = sample_movement("m-dup-2", "receipt-dup", 1_700_000_001);
        let error = db
            .stock_movements()
            .create(&duplicate_movement, &mut NoTransaction)
            .await
            .expect_err("同一业务动作重复入账必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let reservation = sample_reservation("rsv-dup", "10", "sku-2");
        db.stock_reservations()
            .create(&reservation, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_reservation = sample_reservation("rsv-dup-2", "10", "sku-3");
        let error = db
            .stock_reservations()
            .create(&duplicate_reservation, &mut NoTransaction)
            .await
            .expect_err("重复预占建立动作必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let adjustment = sample_adjustment("adj-dup", "ADJ-DUP-001");
        db.stock_adjustments()
            .create(&adjustment, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_adjustment = sample_adjustment("adj-dup-2", "ADJ-DUP-001");
        let error = db
            .stock_adjustments()
            .create(&duplicate_adjustment, &mut NoTransaction)
            .await
            .expect_err("重复 adjustment_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn movement_list_search_respects_filters_pagination_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        for index in 0..3 {
            let movement = sample_movement(
                &format!("m-list-{index}"),
                &format!("receipt-list-{index}"),
                1_700_000_000 + index as i64,
            );
            db.stock_movements()
                .create(&movement, &mut NoTransaction)
                .await
                .unwrap();
        }

        let filter = StockMovementFilter {
            warehouse_id: Some(WarehouseId::new("wh-1")),
            sku_id: Some(SkuId::new("sku-1")),
            movement_type: Some(MovementType::PurchaseReceiptIn),
            direction: None,
            occurred_from: Some(Instant::from_unix_secs(1_700_000_000)),
            occurred_to: Some(Instant::from_unix_secs(1_700_000_001)),
            page: 1,
            page_size: 2,
            sort_by: Some("occurred_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .stock_movements()
            .search_stock_movements(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "时间范围过滤只命中两条");
        assert_eq!(page.items.len(), 2);
        assert_eq!(
            page.items[0].source_document_id, "receipt-list-0",
            "occurred_at 升序"
        );
        let row = &page.items[0];
        assert_eq!(row.movement_type, MovementType::PurchaseReceiptIn);
        assert_eq!(row.direction, MovementDirection::Increase);
        assert_eq!(row.quantity, Quantity::from_str("10").unwrap());
        assert_eq!(row.occurred_at.unix_secs(), 1_700_000_000);

        let second_page = StockMovementFilter {
            page: 2,
            page_size: 2,
            ..filter
        };
        let next = db
            .stock_movements()
            .search_stock_movements(&second_page, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(next.items.len(), 0, "第二页无剩余（时间范围只有两条）");

        let balance_filter = StockBalanceFilter {
            warehouse_id: Some(WarehouseId::new("wh-1")),
            sku_id: None,
            page: 1,
            page_size: 20,
            sort_by: Some("sku_id".to_string()),
            sort_ascending: true,
        };
        let empty = db
            .stock_balances()
            .search_stock_balances(&balance_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(empty.total, 0, "未建余额时返回空列表");
    })
}

#[tokio::test]
#[ignore]
async fn reservation_and_adjustment_list_search_filters_apply() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_rsv_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.stock_reservations()
            .create(
                &sample_reservation("rsv-list-1", "10", "sku-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.stock_adjustments()
            .create(
                &sample_adjustment("adj-list-1", "ADJ-LIST-001"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let mut submitted = sample_adjustment("adj-list-2", "ADJ-LIST-002");
        submitted.submit_for_warehouse_review("reviewer-1").unwrap();
        db.stock_adjustments()
            .create(&submitted, &mut NoTransaction)
            .await
            .unwrap();

        let reservation_filter = StockReservationFilter {
            warehouse_id: None,
            sku_id: Some(SkuId::new("sku-1")),
            status: Some(ReservationStatus::Active),
            sales_order_line_id: Some(SalesOrderLineId::new("so-line-1")),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let reservations = db
            .stock_reservations()
            .search_stock_reservations(&reservation_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(reservations.total, 1);
        assert_eq!(reservations.items[0].status, ReservationStatus::Active);
        assert_eq!(
            reservations.items[0].reserved_quantity,
            Quantity::from_str("10").unwrap()
        );

        let adjustment_filter = StockAdjustmentFilter {
            warehouse_id: Some(WarehouseId::new("wh-1")),
            status: Some(StockAdjustmentState::PendingWarehouseReview),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let adjustments = db
            .stock_adjustments()
            .search_stock_adjustments(&adjustment_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(adjustments.total, 1);
        assert_eq!(adjustments.items[0].adjustment_no, "ADJ-LIST-002");
        assert_eq!(adjustments.items[0].reviewed_by.as_deref(), Some("reviewer-1"));
    })
}

#[tokio::test]
#[ignore]
async fn balance_atomic_deduct_sufficient_succeeds_insufficient_rejected() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_atomic_deduct").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let balance = sample_balance("b-atomic", "sku-1");
        db.stock_balances()
            .create(&balance, &mut NoTransaction)
            .await
            .unwrap();

        let deducted = db
            .stock_balances()
            .deduct_available("b-atomic", Quantity::from_str("10").unwrap(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(deducted, "可用量 70 足够扣减 10");

        let after = db
            .stock_balances()
            .find_by_id("b-atomic", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.on_hand_quantity, Quantity::from_str("90").unwrap());
        assert_eq!(after.reserved_quantity, Quantity::from_str("30").unwrap());
        assert_eq!(after.available_quantity, Quantity::from_str("60").unwrap());
        assert_eq!(after.base.version, 2, "原子写递增 version");

        let over_deducted = db
            .stock_balances()
            .deduct_available("b-atomic", Quantity::from_str("61").unwrap(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(!over_deducted, "可用量 60 不足以扣减 61，写条件整体拒绝");

        let unchanged = db
            .stock_balances()
            .find_by_id("b-atomic", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            unchanged.on_hand_quantity,
            Quantity::from_str("90").unwrap(),
            "拒绝后文档不变"
        );
        assert_eq!(unchanged.available_quantity, Quantity::from_str("60").unwrap());
        assert_eq!(unchanged.base.version, 2, "拒绝后 version 不变");

        let missing = db
            .stock_balances()
            .deduct_available("b-不存在", Quantity::from_str("1").unwrap(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(!missing, "余额行不存在时返回 false");
    })
}

#[tokio::test]
#[ignore]
async fn balance_atomic_reserve_release_and_increase_keep_invariants() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_atomic_rsv").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let balance = sample_balance("b-atomic-2", "sku-2");
        db.stock_balances()
            .create(&balance, &mut NoTransaction)
            .await
            .unwrap();

        let reserved = db
            .stock_balances()
            .reserve_quantity(
                "b-atomic-2",
                Quantity::from_str("20").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(reserved, "可用量 70 足够预占 20");
        let over_reserved = db
            .stock_balances()
            .reserve_quantity(
                "b-atomic-2",
                Quantity::from_str("51").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(!over_reserved, "剩余可用量 50 不足以预占 51");

        let released = db
            .stock_balances()
            .release_reserved(
                "b-atomic-2",
                Quantity::from_str("50").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(released, "预占 50 可整体释放");
        let over_release = db
            .stock_balances()
            .release_reserved("b-atomic-2", Quantity::from_str("1").unwrap(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(!over_release, "预占已为 0，释放被写条件拒绝");

        let increased = db
            .stock_balances()
            .increase_on_hand(
                "b-atomic-2",
                Quantity::from_str("25.5").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(increased);

        let final_balance = db
            .stock_balances()
            .find_by_id("b-atomic-2", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            final_balance.on_hand_quantity,
            Quantity::from_str("125.5").unwrap()
        );
        assert_eq!(final_balance.reserved_quantity, Quantity::from_str("0").unwrap());
        assert_eq!(
            final_balance.available_quantity,
            Quantity::from_str("125.5").unwrap(),
            "恒等式 available = on_hand - reserved 始终成立"
        );
    })
}

#[tokio::test]
#[ignore]
async fn reservation_atomic_consume_transitions_status_and_rejects_overdraw() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_atomic_consume").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.stock_reservations()
            .create(
                &sample_reservation("rsv-atomic", "10", "sku-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let consumed = db
            .stock_reservations()
            .consume_quantity("rsv-atomic", Quantity::from_str("4").unwrap(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(consumed, "预占 10 足够消耗 4");
        let partial = db
            .stock_reservations()
            .find_by_id("rsv-atomic", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(partial.reserved_quantity, Quantity::from_str("6").unwrap());
        assert_eq!(partial.consumed_quantity, Quantity::from_str("4").unwrap());
        assert_eq!(partial.status, ReservationStatus::PartiallyConsumed);

        let over_consumed = db
            .stock_reservations()
            .consume_quantity("rsv-atomic", Quantity::from_str("7").unwrap(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(!over_consumed, "剩余预占 6 不足以消耗 7，写条件整体拒绝");
        let unchanged = db
            .stock_reservations()
            .find_by_id("rsv-atomic", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            unchanged.reserved_quantity,
            Quantity::from_str("6").unwrap(),
            "拒绝后文档不变"
        );
        assert_eq!(unchanged.status, ReservationStatus::PartiallyConsumed);

        let exhausted = db
            .stock_reservations()
            .consume_quantity("rsv-atomic", Quantity::from_str("6").unwrap(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(exhausted);
        let consumed_doc = db
            .stock_reservations()
            .find_by_id("rsv-atomic", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(consumed_doc.reserved_quantity, Quantity::from_str("0").unwrap());
        assert_eq!(
            consumed_doc.status,
            ReservationStatus::Consumed,
            "剩余归零迁移 CONSUMED"
        );

        let after_exhausted = db
            .stock_reservations()
            .consume_quantity("rsv-atomic", Quantity::from_str("1").unwrap(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(!after_exhausted, "CONSUMED 状态不再可消耗");
    })
}

#[tokio::test]
#[ignore]
async fn reservation_atomic_release_requires_full_remaining() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_atomic_release").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.stock_reservations()
            .create(
                &sample_reservation("rsv-release", "10", "sku-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let partial_release = db
            .stock_reservations()
            .release_quantity(
                "rsv-release",
                Quantity::from_str("5").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(
            !partial_release,
            "剩余预占 10 != 5，部分释放不构成合法状态，整体拒绝"
        );
        let unchanged = db
            .stock_reservations()
            .find_by_id("rsv-release", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.reserved_quantity, Quantity::from_str("10").unwrap());
        assert_eq!(unchanged.status, ReservationStatus::Active);

        let full_release = db
            .stock_reservations()
            .release_quantity(
                "rsv-release",
                Quantity::from_str("10").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(full_release, "全额释放命中写条件");
        let released_doc = db
            .stock_reservations()
            .find_by_id("rsv-release", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(released_doc.reserved_quantity, Quantity::from_str("0").unwrap());
        assert_eq!(released_doc.released_quantity, Quantity::from_str("10").unwrap());
        assert_eq!(released_doc.status, ReservationStatus::Released);
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_commit_makes_adjustment_and_lines_atomically_visible() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_tx_ok").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let adjustment = sample_adjustment("adj-tx-ok", "ADJ-TX-001");
        let lines = vec![sample_adjustment_line("al-tx-ok", "adj-tx-ok", "sku-1")];
        let db_clone = db.clone();
        let adjustment_for_tx = adjustment.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .inventory()
                        .create_stock_adjustment_with_lines(&adjustment_for_tx, &lines, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        assert!(db
            .stock_adjustments()
            .find_by_id(&adjustment.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(db
            .stock_adjustment_lines()
            .find_by_id("al-tx-ok", &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_adjustment_and_lines_together() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let adjustment = sample_adjustment("adj-tx-abort", "ADJ-TX-002");
        let lines = vec![sample_adjustment_line("al-tx-abort", "adj-tx-abort", "sku-1")];
        let db_clone = db.clone();
        let adjustment_for_tx = adjustment.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .inventory()
                        .create_stock_adjustment_with_lines(&adjustment_for_tx, &lines, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        assert!(
            db.stock_adjustments()
                .find_by_id(&adjustment.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后表头不得残留"
        );
        assert!(
            db.stock_adjustment_lines()
                .find_by_id("al-tx-abort", &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后明细不得残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_with_no_transaction_writes_are_predictable() {
    require_mongo!(async {
        let test_db = TestDb::new("b6g6_inv_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let adjustment = sample_adjustment("adj-notx", "ADJ-NOTX-001");
        let lines = vec![sample_adjustment_line("al-notx", "adj-notx", "sku-1")];
        db.inventory()
            .create_stock_adjustment_with_lines(&adjustment, &lines, &mut NoTransaction)
            .await
            .unwrap();

        assert!(db
            .stock_adjustments()
            .find_by_id(&adjustment.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(db
            .stock_adjustment_lines()
            .find_by_id("al-notx", &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
    })
}
