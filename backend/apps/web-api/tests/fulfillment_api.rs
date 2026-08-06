//! 域 D16 `fulfillment` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test fulfillment_api -- --include-ignored`。
//!
//! 覆盖 §8.2 第 1/2/5 条与 §8.1.5 事务不变量：入库过账「入库单 + 库存流水 +
//! 余额 + 销售预占 + 采购进度」同时生效；仓发过账「发货单 + 预占消耗 + 出库
//! 流水 + 余额」同时生效；验收过账「验收单 + APPLY 分配」原子可见；注入失败
//! 全部不可见；重复过账返回 409；`PREPAY` 门槛按有效已过账付款净核销金额判定。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use database::{FulfillmentExt, InventoryExt, NoTransaction, PayableExt, PurchaseOrderExt, SalesOrderExt};
use entities::common::time::Instant;
use entities::ids::{
    CustomerAccountId, PartyId, PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId,
    SalesOrderId, SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId, SkuId, StockBalanceId,
    StockReservationId, SupplierAccountId, SupplierCommercialProfileRevisionId, WarehouseId,
};
use mongodb::bson::{doc, Bson, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节，同时满足 config 校验与 test-support 签发要求）。
const TEST_JWT_SECRET: &str = "p0-5-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("purchase_receipt", "list"),
    ("purchase_receipt", "detail"),
    ("purchase_receipt", "create"),
    ("purchase_receipt", "update"),
    ("purchase_receipt", "post"),
    ("delivery", "list"),
    ("delivery", "detail"),
    ("delivery", "create"),
    ("delivery", "update"),
    ("delivery", "post"),
    ("electronic_delivery", "list"),
    ("electronic_delivery", "create"),
    ("electronic_delivery", "confirm"),
    ("service_fulfillment", "list"),
    ("service_fulfillment", "create"),
    ("service_fulfillment", "confirm"),
    ("customer_acceptance", "list"),
    ("customer_acceptance", "detail"),
    ("customer_acceptance", "create"),
    ("customer_acceptance", "post"),
    ("customer_acceptance", "reverse"),
];

/// 为种子账号插入本域直接 `p` 规则（casbin `g(sub, sub)` 自反匹配，无需角色）。
async fn grant_domain_permissions(db: &Database, account_id: &str) {
    let subject = format!("user:admin:{account_id}");
    let rules: Vec<Document> = DOMAIN_PERMISSIONS
        .iter()
        .map(|(resource, action)| {
            let values = vec![subject.clone(), (*resource).to_string(), (*action).to_string()];
            let id = format!("p\u{1f}p\u{1f}{}", values.join("\u{1f}"));
            doc! { "_id": id, "sec": "p", "ptype": "p", "values": values }
        })
        .collect();
    db.collection::<Document>("casbin_rules")
        .insert_many(rules)
        .await
        .expect("插入本域权限规则失败");
}

/// 构造最小 AppState（默认配置 + 临时上传目录）并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "p0-5-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("p0-5-uploads-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&upload_path)
        .await
        .expect("创建临时上传目录失败");
    let state = AppState::new(test_db.db().clone(), SafeConfig::new(config), upload_path.clone());
    (routes::create(state), upload_path)
}

/// 断言响应是统一信封且业务成功。
fn assert_ok_envelope(status: u16, body: &Value) {
    assert_eq!(status, 200, "期望 200，实际 {status}: {body}");
    assert_eq!(body["status"], 200);
    assert_eq!(body["errorMessage"], "OK");
    assert_eq!(body["success"], true);
}

/// 种子：销售单（草稿）与销售版本行（预占归属映射用）。
async fn seed_sales_order(db: &Database) -> (SalesOrderId, SalesOrderRevisionLineId) {
    use entities::common::time::Instant as T;
    use entities::money::Amount;
    use entities::sales_order::{
        BusinessType, LineType, OriginSystem, SalesOrder, SalesOrderData, SalesOrderRevisionLine,
        SalesOrderRevisionLineData,
    };
    use std::str::FromStr;

    let so_id = SalesOrderId::new("so-1");
    let so = SalesOrder::new(
        so_id.clone(),
        SalesOrderData {
            order_no: "SO-2026-001".to_string(),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: None,
            settlement_party_id: PartyId::new("party-1"),
            source_status_code: None,
        },
        "admin-1",
    )
    .unwrap();
    db.sales_orders().create(&so, &mut NoTransaction).await.unwrap();
    let so_revision_id = SalesOrderRevisionId::new("so-rev-1");
    let revision_line = SalesOrderRevisionLine::new(
        SalesOrderRevisionLineId::new("sorl-1"),
        SalesOrderRevisionLineData {
            sales_order_revision_id: so_revision_id.clone(),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            line_no: 1,
            line_type: LineType::GoodsService,
            gross_amount: Amount::from_str("100.00").unwrap(),
            net_amount: Amount::from_str("87.00").unwrap(),
            tax_amount: Amount::from_str("13.00").unwrap(),
            sales_tax_rate: entities::money::Rate::from_str("0.130000").unwrap(),
            item_name_snapshot: "测试商品".to_string(),
            spec_snapshot: Some("规格A".to_string()),
            unit_snapshot: Some("PCS".to_string()),
        },
    )
    .unwrap();
    db.sales_order_revision_lines()
        .create(&revision_line, &mut NoTransaction)
        .await
        .unwrap();
    let _ = T::now();
    (so_id, SalesOrderRevisionLineId::new("sorl-1"))
}

/// 种子：已生效采购单 + 生效版本 + 版本行（可带 `PREPAY` 门槛）。
async fn seed_po(
    db: &Database,
    prepay_gate: bool,
) -> (
    PurchaseOrderId,
    PurchaseOrderRevisionId,
    PurchaseOrderRevisionLineId,
    SkuId,
) {
    use entities::common::time::Instant as T;
    use entities::money::{Amount, Quantity, Rate, UnitPrice};
    use entities::purchase_order::{
        FulfillmentResponsibility, PaymentTermSnapshot, ProgressStatus, PurchaseLineType, PurchaseOrder,
        PurchaseOrderData, PurchaseOrderRevision, PurchaseOrderRevisionData, PurchaseOrderRevisionLine,
        PurchaseOrderRevisionLineData, PurchaseOrderStatus, PurchaseType, SupplierSnapshot,
    };
    use std::str::FromStr;

    let po_id = PurchaseOrderId::new("po-1");
    let mut po = PurchaseOrder::new(
        po_id.clone(),
        PurchaseOrderData {
            purchase_no: "PO-2026-001".to_string(),
            sales_order_id: SalesOrderId::new("so-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            payment_term_code: if prepay_gate {
                "PREPAY-100".to_string()
            } else {
                "NET-30".to_string()
            },
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
        },
        "admin-1",
    )
    .unwrap();
    po.submit_for_review("sub-1", "admin-1").unwrap();
    po.apply_finance_review(true, "fin-1").unwrap();
    assert_eq!(po.stable.status, PurchaseOrderStatus::Effective);
    db.purchase_orders()
        .create(&po, &mut NoTransaction)
        .await
        .unwrap();

    let revision_id = PurchaseOrderRevisionId::new("po-rev-1");
    let revision = PurchaseOrderRevision::new(
        revision_id.clone(),
        PurchaseOrderRevisionData {
            purchase_order_id: po_id.clone(),
            revision_no: 1,
            supplier_revision_id: SupplierCommercialProfileRevisionId::new("scpr-1"),
            supplier_snapshot: SupplierSnapshot::new("测试供应商".to_string()).unwrap(),
            payment_term_snapshot: PaymentTermSnapshot::new(
                if prepay_gate {
                    "PREPAY-100".to_string()
                } else {
                    "NET-30".to_string()
                },
                prepay_gate,
                prepay_gate.then(|| Amount::from_str("100.00").unwrap()),
                None,
            )
            .unwrap(),
            gross_amount: Amount::from_str("1000.00").unwrap(),
            net_amount: Amount::from_str("870.00").unwrap(),
            tax_amount: Amount::from_str("130.00").unwrap(),
            effective_at: T::now(),
        },
    )
    .unwrap();
    db.purchase_order_revisions()
        .create(&revision, &mut NoTransaction)
        .await
        .unwrap();
    let revision_line_id = PurchaseOrderRevisionLineId::new("porl-1");
    let revision_line = PurchaseOrderRevisionLine::new(
        revision_line_id.clone(),
        PurchaseOrderRevisionLineData {
            purchase_order_revision_id: revision_id.clone(),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(entities::ids::ProcurementConfirmationLineId::new(
                "pcl-1",
            )),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: None,
            product_name_snapshot: Some("测试商品".to_string()),
            specification_snapshot: Some("规格A".to_string()),
            quantity: Some(Quantity::from_str("10").unwrap()),
            base_unit_code: Some("PCS".to_string()),
            unit_cost_gross: Some(UnitPrice::from_str("100.0000").unwrap()),
            gross_amount: Amount::from_str("1000.00").unwrap(),
            net_amount: Amount::from_str("870.00").unwrap(),
            tax_amount: Amount::from_str("130.00").unwrap(),
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: None,
        },
    )
    .unwrap();
    db.purchase_order_revision_lines()
        .create(&revision_line, &mut NoTransaction)
        .await
        .unwrap();
    // 版本指针 + 履约进度（模拟 D15 P3 生效后的主表状态）
    db.collection::<Document>("purchase_orders")
        .update_one(
            doc! { "id": po_id.to_string() },
            doc! { "$set": { "current_revision_id": revision_id.to_string(), "fulfillment_progress": ProgressStatus::None.as_str() } },
        )
        .await
        .unwrap();
    (po_id, revision_id, revision_line_id, SkuId::new("sku-1"))
}

/// 种子：采购销售分配（`PurchaseLineSalesAllocationData` 未从 entities 导出，
/// 测试按集合文档形态直接写入）。
async fn seed_allocation_raw(
    db: &Database,
    revision_line_id: &PurchaseOrderRevisionLineId,
    sales_revision_line_id: &SalesOrderRevisionLineId,
) {
    let collection = db.collection::<Document>("purchase_line_sales_allocations");
    let now = Instant::now().unix_secs();
    let quantity = decimal128_bson("10.000000");
    collection
        .insert_one(doc! {
            "id": "pla-1",
            "version": 1,
            "created_at": now,
            "updated_at": now,
            "deleted_at": 0,
            "purchase_order_revision_line_id": revision_line_id.to_string(),
            "sales_order_revision_line_id": sales_revision_line_id.to_string(),
            "allocated_quantity": quantity,
            "allocated_cost_gross": decimal128_bson("1000.00"),
            "allocated_cost_net": decimal128_bson("870.00"),
        })
        .await
        .unwrap();
}

/// 把十进制字符串转 Decimal128 BSON（非人性化序列化，与仓储 `$inc` 形态一致）。
#[allow(deprecated)]
fn decimal128_bson(value: &str) -> Bson {
    mongodb::bson::to_bson_with_options(
        &value,
        mongodb::bson::SerializerOptions::builder()
            .human_readable(false)
            .build(),
    )
    .unwrap()
}

/// 种子：`PREPAY` 门槛所需的有效已过账付款（应付子账 + 分录 + 付款核销分配）。
async fn seed_payable_paid(db: &Database, po_id: &PurchaseOrderId, amount: &str) {
    use entities::common::time::BusinessDate;
    use entities::money::Amount;
    use entities::payable::{
        AllocationAction as PayableAllocationAction, PayableAccount, PayableAccountData, PayableEntry,
        PayableEntryData, PayableEntryType, PayableSourceType, PaymentAllocation, PaymentAllocationData,
        SupplierPayment, SupplierPaymentData,
    };
    use std::str::FromStr;

    let account = PayableAccount::new(
        entities::ids::PayableAccountId::new("payable-acc-1"),
        PayableAccountData {
            source_document_id: po_id.to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            source_type: PayableSourceType::PurchaseOrder,
            gross_total: Amount::from_str("1000.00").unwrap(),
            settled_total: Amount::from_str("120.00").unwrap(),
            invoiceable_total: Amount::from_str("1000.00").unwrap(),
            invoiced_total: Amount::from_str("0.00").unwrap(),
        },
        "admin-1",
    )
    .unwrap();
    db.payable_accounts()
        .create(&account, &mut NoTransaction)
        .await
        .unwrap();

    let entry = PayableEntry::new(
        entities::ids::PayableEntryId::new("payable-entry-1"),
        PayableEntryData {
            payable_account_id: entities::ids::PayableAccountId::new("payable-acc-1"),
            entry_type: PayableEntryType::Original,
            direction: entities::payable::EntryDirection::Increase,
            amount: Amount::from_str("1000.00").unwrap(),
            due_date: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
            source_fact_type: "purchase_order".to_string(),
            source_document_id: po_id.to_string(),
            source_revision_id: "po-rev-1".to_string(),
            source_sequence: 1,
            posted_at: Instant::now(),
        },
    )
    .unwrap();
    db.payable_entries()
        .create(&entry, &mut NoTransaction)
        .await
        .unwrap();

    let payment = SupplierPayment::new(
        entities::ids::SupplierPaymentId::new("payment-1"),
        SupplierPaymentData {
            payment_no: "PAY-2026-001".to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            paid_at: Instant::now(),
            amount: Amount::from_str(amount).unwrap(),
            bank_reference: None,
        },
    )
    .unwrap();
    db.supplier_payments()
        .create(&payment, &mut NoTransaction)
        .await
        .unwrap();
    let allocation = PaymentAllocation::new(
        entities::ids::PaymentAllocationId::new("pay-alloc-1"),
        PaymentAllocationData {
            supplier_payment_id: entities::ids::SupplierPaymentId::new("payment-1"),
            payable_entry_id: entities::ids::PayableEntryId::new("payable-entry-1"),
            allocation_seq: 1,
            allocation_action: PayableAllocationAction::Apply,
            allocated_amount: Amount::from_str(amount).unwrap(),
            allocated_at: Instant::now(),
            reverses_allocation_id: None,
        },
    )
    .unwrap();
    db.payment_allocations()
        .create(&allocation, &mut NoTransaction)
        .await
        .unwrap();
}

/// 种子：库存余额 + 有效预占（仓发过账前置）。
async fn seed_balance_with_reservation(db: &Database) {
    use entities::inventory::{
        ReservationStatus, StockBalance, StockBalanceData, StockReservation, StockReservationData,
    };
    use entities::money::Quantity;
    use std::str::FromStr;

    let balance = StockBalance::new(
        StockBalanceId::new("bal-1"),
        StockBalanceData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            on_hand_quantity: Quantity::from_str("10").unwrap(),
            reserved_quantity: Quantity::from_str("4").unwrap(),
            available_quantity: Quantity::from_str("6").unwrap(),
            last_movement_id: None,
        },
    )
    .unwrap();
    db.stock_balances()
        .create(&balance, &mut NoTransaction)
        .await
        .unwrap();
    let reservation = StockReservation::new(
        StockReservationId::new("rsv-1"),
        StockReservationData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_line_sales_allocation_id: entities::ids::PurchaseLineSalesAllocationId::new("pla-1"),
            source_receipt_line_id: entities::ids::PurchaseReceiptLineId::new("rline-1"),
            reserved_quantity: Quantity::from_str("4").unwrap(),
            consumed_quantity: Quantity::from_str("0").unwrap(),
            released_quantity: Quantity::from_str("0").unwrap(),
            status: ReservationStatus::Active,
        },
    )
    .unwrap();
    db.stock_reservations()
        .create(&reservation, &mut NoTransaction)
        .await
        .unwrap();
}

/// 种子：一条已过账的仓发货单（验收过账的事实来源）。
async fn seed_posted_delivery_fact(db: &Database) -> String {
    use entities::fulfillment::{
        Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryState, DeliveryType,
    };
    use entities::money::Quantity;
    use std::str::FromStr;

    let delivery = Delivery::new(
        entities::ids::DeliveryId::new("delivery-1"),
        DeliveryData {
            delivery_no: "DV-2026-001".to_string(),
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
    .unwrap();
    let line = DeliveryLine::new(
        entities::ids::DeliveryLineId::new("dl-1"),
        DeliveryLineData {
            delivery_id: entities::ids::DeliveryId::new("delivery-1"),
            line_no: 1,
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            quantity: Quantity::from_str("4").unwrap(),
            stock_reservation_id: Some(StockReservationId::new("rsv-1")),
            purchase_line_sales_allocation_id: None,
        },
        DeliveryType::WarehouseShip,
    )
    .unwrap();
    let mut delivery = delivery;
    delivery.mark_shipped(Instant::now()).unwrap();
    // 直接写入已发货状态（测试种子；业务路径由 delivery_post 覆盖）
    db.fulfillment()
        .create_delivery_with_lines(&delivery, &[line], &mut NoTransaction)
        .await
        .unwrap();
    assert_eq!(delivery.status, DeliveryState::Shipped);
    "delivery-1".to_string()
}

#[tokio::test]
#[ignore]
async fn receipt_post_posts_inventory_reservation_and_po_progress() {
    require_mongo!(async {
        let test_db = TestDb::new("ff_api_receipt").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_sales_order(test_db.db()).await;
        let (_, _, revision_line_id, _) = seed_po(test_db.db(), false).await;
        seed_allocation_raw(
            test_db.db(),
            &revision_line_id,
            &SalesOrderRevisionLineId::new("sorl-1"),
        )
        .await;

        let (status, body) = api
            .post(
                "/admin/purchase-receipts",
                Some(&token),
                Some(json!({
                    "receipt_no": "RK-2026-001",
                    "purchase_order_id": "po-1",
                    "warehouse_id": "wh-1",
                    "lines": [{
                        "purchase_order_revision_line_id": revision_line_id.to_string(),
                        "received_quantity": "10",
                        "qualified_quantity": "9",
                        "rejected_quantity": "1"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let receipt_id = body["data"]["id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["status"], "DRAFT");
        assert_eq!(body["data"]["version"], 1);

        let (status, body) = api
            .post(
                &format!("/admin/purchase-receipts/{receipt_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "POSTED");
        assert!(body["data"]["posted_at"].as_i64().unwrap() > 0);

        // §8.2 第 1 条不变量：入库单 + 流水 + 余额 + 预占 + 采购进度同时生效
        let movements = test_db
            .db()
            .stock_movements()
            .find_by_source_document(&receipt_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(movements.len(), 1, "仅合格数量形成流水");
        assert_eq!(movements[0].movement_type.as_str(), "PURCHASE_RECEIPT_IN");
        assert_eq!(movements[0].quantity.to_string(), "9.000000");
        assert_eq!(movements[0].direction.as_str(), "INCREASE");

        let balance = test_db
            .db()
            .stock_balances()
            .find_by_dimensions(
                &WarehouseId::new("wh-1"),
                &SkuId::new("sku-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("余额必须存在");
        assert_eq!(balance.on_hand_quantity.to_string(), "9.000000");
        assert_eq!(
            balance.reserved_quantity.to_string(),
            "9.000000",
            "合格数量全部预占"
        );
        assert_eq!(balance.available_quantity.to_string(), "0.000000");

        let reservation = test_db
            .db()
            .stock_reservations()
            .find_many(
                doc! { "source_receipt_line_id": { "$exists": true } },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(reservation.len(), 1, "沿采购销售分配建立预占");
        assert_eq!(reservation[0].reserved_quantity.to_string(), "9.000000");
        assert_eq!(reservation[0].status.as_str(), "ACTIVE");

        let po = test_db
            .db()
            .purchase_orders()
            .find_by_id("po-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(po.fulfillment_progress.as_str(), "PARTIAL", "采购履约进度推进");

        // 重复过账 → 409，且不产生第二条流水
        let (status, body) = api
            .post(
                &format!("/admin/purchase-receipts/{receipt_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(status, 409, "重复过账必须 409: {body}");
        let movements = test_db
            .db()
            .stock_movements()
            .find_by_source_document(&receipt_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(movements.len(), 1);

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn receipt_over_receipt_injection_failure_is_invisible() {
    require_mongo!(async {
        let test_db = TestDb::new("ff_api_over").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_sales_order(test_db.db()).await;
        let (_, _, revision_line_id, _) = seed_po(test_db.db(), false).await;
        seed_allocation_raw(
            test_db.db(),
            &revision_line_id,
            &SalesOrderRevisionLineId::new("sorl-1"),
        )
        .await;

        let (_, body) = api
            .post(
                "/admin/purchase-receipts",
                Some(&token),
                Some(json!({
                    "receipt_no": "RK-2026-OVER",
                    "purchase_order_id": "po-1",
                    "warehouse_id": "wh-1",
                    "lines": [{
                        "purchase_order_revision_line_id": revision_line_id.to_string(),
                        "received_quantity": "12",
                        "qualified_quantity": "11",
                        "rejected_quantity": "1"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let receipt_id = body["data"]["id"].as_str().unwrap().to_string();

        // 注入失败：合格 11 + 不合格 1 = 12 > 采购数量 10 → 400
        let (status, body) = api
            .post(
                &format!("/admin/purchase-receipts/{receipt_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(status, 400, "超收必须 400: {body}");

        // 全部不可见：入库单仍草稿、无流水、无余额、无预占、采购进度未推进
        let receipt = test_db
            .db()
            .purchase_receipts()
            .find_by_id(&receipt_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(receipt.status.as_str(), "DRAFT");
        let movements = test_db
            .db()
            .stock_movements()
            .find_by_source_document(&receipt_id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(movements.is_empty());
        let balance = test_db
            .db()
            .stock_balances()
            .find_by_dimensions(
                &WarehouseId::new("wh-1"),
                &SkuId::new("sku-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(balance.is_none(), "注入失败后不得留下余额");
        let reservations = test_db
            .db()
            .stock_reservations()
            .find_many(doc! {}, &mut NoTransaction)
            .await
            .unwrap();
        assert!(reservations.is_empty(), "注入失败后不得留下预占");
    })
}

#[tokio::test]
#[ignore]
async fn prepay_gate_blocks_until_effective_payment_satisfied() {
    require_mongo!(async {
        let test_db = TestDb::new("ff_api_gate").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_sales_order(test_db.db()).await;
        let (po_id, _, revision_line_id, _) = seed_po(test_db.db(), true).await;
        seed_allocation_raw(
            test_db.db(),
            &revision_line_id,
            &SalesOrderRevisionLineId::new("sorl-1"),
        )
        .await;

        let (_, body) = api
            .post(
                "/admin/purchase-receipts",
                Some(&token),
                Some(json!({
                    "receipt_no": "RK-2026-GATE",
                    "purchase_order_id": po_id.to_string(),
                    "warehouse_id": "wh-1",
                    "lines": [{
                        "purchase_order_revision_line_id": revision_line_id.to_string(),
                        "received_quantity": "10",
                        "qualified_quantity": "9",
                        "rejected_quantity": "1"
                    }]
                })),
            )
            .await;
        let receipt_id = body["data"]["id"].as_str().unwrap().to_string();

        // §8.1.5：先款未到 → 400 且无任何事实
        let (status, body) = api
            .post(
                &format!("/admin/purchase-receipts/{receipt_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(status, 400, "先款未达门槛必须 400: {body}");
        assert!(body["errorMessage"].as_str().unwrap().contains("先款"), "{body}");

        // 有效付款 120 ≥ 门槛 100 → 过账成功
        seed_payable_paid(test_db.db(), &po_id, "120.00").await;
        let (status, body) = api
            .post(
                &format!("/admin/purchase-receipts/{receipt_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "POSTED");
        let balance = test_db
            .db()
            .stock_balances()
            .find_by_dimensions(
                &WarehouseId::new("wh-1"),
                &SkuId::new("sku-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("门槛满足后余额必须形成");
        assert_eq!(balance.on_hand_quantity.to_string(), "9.000000");
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_ship_post_consumes_reservation_and_deducts_balance() {
    require_mongo!(async {
        let test_db = TestDb::new("ff_api_ship").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_balance_with_reservation(test_db.db()).await;

        let (status, body) = api
            .post(
                "/admin/deliveries",
                Some(&token),
                Some(json!({
                    "delivery_no": "DV-2026-002",
                    "delivery_type": "WAREHOUSE_SHIP",
                    "sales_order_id": "so-1",
                    "warehouse_id": "wh-1",
                    "carrier": "顺丰",
                    "tracking_no": "SF-002",
                    "lines": [{
                        "sales_order_line_id": "so-line-1",
                        "quantity": "3",
                        "stock_reservation_id": "rsv-1"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let delivery_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/deliveries/{delivery_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "SHIPPED");
        assert!(body["data"]["shipped_at"].as_i64().unwrap() > 0);

        // §8.2 第 2 条不变量：预占消耗 + 出库流水 + 余额同步
        let reservation = test_db
            .db()
            .stock_reservations()
            .find_by_id("rsv-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reservation.status.as_str(), "PARTIALLY_CONSUMED");
        assert_eq!(reservation.consumed_quantity.to_string(), "3.000000");
        assert_eq!(reservation.reserved_quantity.to_string(), "1.000000");

        let balance = test_db
            .db()
            .stock_balances()
            .find_by_dimensions(
                &WarehouseId::new("wh-1"),
                &SkuId::new("sku-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(balance.on_hand_quantity.to_string(), "7.000000", "10 − 3");
        assert_eq!(balance.reserved_quantity.to_string(), "1.000000", "4 − 3");
        assert_eq!(balance.available_quantity.to_string(), "6.000000");

        let movements = test_db
            .db()
            .stock_movements()
            .find_by_source_document(&delivery_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(movements.len(), 1);
        assert_eq!(movements[0].movement_type.as_str(), "WAREHOUSE_SHIP_OUT");
        assert_eq!(movements[0].direction.as_str(), "DECREASE");

        // 重复过账 → 409
        let (status, body) = api
            .post(
                &format!("/admin/deliveries/{delivery_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(status, 409, "重复过账必须 409: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn acceptance_post_writes_allocations_and_reverse_creates_reverse_facts() {
    require_mongo!(async {
        let test_db = TestDb::new("ff_api_accept").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_sales_order(test_db.db()).await;
        seed_balance_with_reservation(test_db.db()).await;
        seed_posted_delivery_fact(test_db.db()).await;

        let (status, body) = api
            .post(
                "/admin/customer-acceptances",
                Some(&token),
                Some(json!({
                    "acceptance_no": "CA-2026-001",
                    "sales_order_id": "so-1",
                    "accepted_at": 1754438400,
                    "result": "PASSED",
                    "lines": [{
                        "sales_order_line_id": "so-line-1",
                        "accepted_quantity": "3",
                        "short_quantity": "1",
                        "rejected_quantity": "0",
                        "reason": "部分短少"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let acceptance_id = body["data"]["id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["status"], "DRAFT");

        // 过账分配不守恒 → 400（§8.2 第 5 条）
        let (status, body) = api
            .post(
                &format!("/admin/customer-acceptances/{acceptance_id}/post"),
                Some(&token),
                Some(json!({
                    "lines": [{
                        "sales_order_line_id": "so-line-1",
                        "allocations": [{
                            "fulfillment_line_id": "dl-1",
                            "fulfillment_fact_type": "DELIVERY",
                            "allocated_quantity": "2"
                        }]
                    }]
                })),
            )
            .await;
        assert_eq!(status, 400, "分配合计不等于通过数量必须 400: {body}");

        // 守恒后过账成功：验收单 + APPLY 分配原子可见
        let (status, body) = api
            .post(
                &format!("/admin/customer-acceptances/{acceptance_id}/post"),
                Some(&token),
                Some(json!({
                    "lines": [{
                        "sales_order_line_id": "so-line-1",
                        "allocations": [{
                            "fulfillment_line_id": "dl-1",
                            "fulfillment_fact_type": "DELIVERY",
                            "allocated_quantity": "3"
                        }]
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "POSTED");

        let detail = api
            .get(
                &format!("/admin/customer-acceptances/{acceptance_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(detail.0, &detail.1);
        assert_eq!(detail.1["data"]["allocations"].as_array().unwrap().len(), 1);
        assert_eq!(detail.1["data"]["allocations"][0]["allocation_action"], "APPLY");
        assert_eq!(detail.1["data"]["allocations"][0]["fulfillment_line_id"], "dl-1");

        // 重复过账 → 409
        let (status, _) = api
            .post(
                &format!("/admin/customer-acceptances/{acceptance_id}/post"),
                Some(&token),
                Some(json!({
                    "lines": [{
                        "sales_order_line_id": "so-line-1",
                        "allocations": [{
                            "fulfillment_line_id": "dl-1",
                            "fulfillment_fact_type": "DELIVERY",
                            "allocated_quantity": "3"
                        }]
                    }]
                })),
            )
            .await;
        assert_eq!(status, 409);

        // 冲正：反向验收 + REVERSE 分配；原验收 REVERSED
        let (status, body) = api
            .post(
                &format!("/admin/customer-acceptances/{acceptance_id}/reverse"),
                Some(&token),
                Some(json!({ "expected_version": 1, "reason_text": "误录，冲正" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let reverse_id = body["data"]["id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["status"], "POSTED");

        let original = test_db
            .db()
            .customer_acceptances()
            .find_by_id(&acceptance_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(original.status.as_str(), "REVERSED");
        assert_eq!(original.reversal_of_acceptance_id.unwrap().as_ref(), reverse_id);

        let reverse_detail = api
            .get(&format!("/admin/customer-acceptances/{reverse_id}"), Some(&token))
            .await;
        assert_ok_envelope(reverse_detail.0, &reverse_detail.1);
        let allocations = reverse_detail.1["data"]["allocations"].as_array().unwrap();
        assert_eq!(allocations.len(), 1);
        assert_eq!(allocations[0]["allocation_action"], "REVERSE");
        assert_eq!(
            allocations[0]["reverses_allocation_id"].as_str().unwrap().len(),
            32
        );

        // 验收历史与工作台（W06）形状
        let (status, body) = api
            .get(
                "/admin/customer-acceptances/eligible?sales_order_id=so-1",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["sales_order_id"], "so-1");
        let sales_lines = body["data"]["sales_lines"].as_array().unwrap();
        assert!(!sales_lines.is_empty(), "至少一个销售行分组");
        let facts = sales_lines[0]["fulfillment_facts"].as_array().unwrap();
        assert!(!facts.is_empty(), "存在可验收事实");
        assert_eq!(
            facts[0]["net_accepted_allocated_quantity"], "0.000000",
            "APPLY 3 − REVERSE 3 = 0"
        );
        assert_eq!(
            facts[0]["eligible_quantity"], "4.000000",
            "净成功 4 − 净已验收 0 = 4"
        );
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("ff_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/deliveries", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("ff_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/deliveries", Some(&token)).await;
        assert_eq!(status, 403, "无 delivery.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("ff_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/deliveries",
                Some(&token),
                Some(json!({
                    "delivery_no": "  ",
                    "delivery_type": "WAREHOUSE_SHIP",
                    "sales_order_id": "so-1",
                    "lines": []
                })),
            )
            .await;
        assert_eq!(status, 400, "空白单号必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, _) = api
            .post(
                "/admin/deliveries",
                Some(&token),
                Some(json!({ "delivery_no": "DV-X" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/deliveries",
                Some(&token),
                Some(json!({
                    "delivery_no": "DV-X",
                    "delivery_type": "MARS",
                    "sales_order_id": "so-1",
                    "lines": []
                })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");

        // 排序白名单在 Service 层校验 → 400
        let (status, body) = api.get("/admin/deliveries?sort_by=quantity", Some(&token)).await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_boundary_for_deliveries_and_receipts() {
    require_mongo!(async {
        let test_db = TestDb::new("ff_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        for index in 1..=2 {
            let (_, body) = api
                .post(
                    "/admin/deliveries",
                    Some(&token),
                    Some(json!({
                        "delivery_no": format!("DV-2026-00{index}"),
                        "delivery_type": "WAREHOUSE_SHIP",
                        "sales_order_id": "so-1",
                        "warehouse_id": "wh-1",
                        "lines": [{
                            "sales_order_line_id": "so-line-1",
                            "quantity": "1",
                            "stock_reservation_id": "rsv-1"
                        }]
                    })),
                )
                .await;
            assert_ok_envelope(200, &body);
        }

        let (status, body) = api
            .get(
                "/admin/deliveries?page=1&page_size=1&sort_by=created_at&sort_dir=desc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);

        let (status, body) = api
            .get("/admin/deliveries?page=1&page_size=0", Some(&token))
            .await;
        assert_eq!(status, 400, "page_size=0 必须 400: {body}");
    })
}
