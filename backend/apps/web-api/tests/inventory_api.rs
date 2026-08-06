//! 域 D17 `inventory` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test inventory_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入本域直接 `p` 规则
//! （casbin 的 `g(r.sub, p.sub)` 自反匹配，无需改角色）。
//!
//! 覆盖 §8.2 第 3/4 条事务不变量：过账成功时「调整单 + 库存流水 + 余额」
//! 同时生效；注入失败后全部不可见；重复过账返回 409。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use database::InventoryExt;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节，同时满足 config 校验与 test-support 签发要求）。
const TEST_JWT_SECRET: &str = "p0-5-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("stock_balance", "list"),
    ("stock_balance", "detail"),
    ("stock_movement", "list"),
    ("stock_reservation", "list"),
    ("stock_adjustment", "list"),
    ("stock_adjustment", "detail"),
    ("stock_adjustment", "create"),
    ("stock_adjustment", "update"),
    ("stock_adjustment", "submit"),
    ("stock_adjustment", "approve"),
    ("stock_adjustment", "reject"),
    ("stock_adjustment", "post"),
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

/// 直接写入库存余额（测试种子：跳过 HTTP 层，聚焦过账不变量）。
async fn seed_balance(db: &Database, id: &str, warehouse: &str, sku: &str, on_hand: &str) {
    use entities::ids::{SkuId, StockBalanceId, WarehouseId};
    use entities::inventory::{StockBalance, StockBalanceData};
    use entities::money::Quantity;
    use std::str::FromStr;

    let balance = StockBalance::new(
        StockBalanceId::new(id),
        StockBalanceData {
            warehouse_id: WarehouseId::new(warehouse),
            sku_id: SkuId::new(sku),
            on_hand_quantity: Quantity::from_str(on_hand).unwrap(),
            reserved_quantity: Quantity::from_str("0").unwrap(),
            available_quantity: Quantity::from_str(on_hand).unwrap(),
            last_movement_id: None,
        },
    )
    .unwrap();
    db.stock_balances()
        .create(&balance, &mut database::NoTransaction)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn happy_path_adjustment_flow_posts_movement_and_balance() {
    require_mongo!(async {
        let test_db = TestDb::new("inv_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/stock-balances", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"], json!([]), "初始余额为空");
        assert_eq!(body["data"]["page"], 1);
        assert_eq!(body["data"]["page_size"], 20);

        let (status, body) = api.get("/admin/stock-adjustments", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0);

        let (status, body) = api
            .post(
                "/admin/stock-adjustments",
                Some(&token),
                Some(json!({
                    "adjustment_no": "ADJ-2026-001",
                    "warehouse_id": "wh-1",
                    "reason_type": "STOCK_GAIN",
                    "lines": [{ "sku_id": "sku-1", "quantity": "3", "direction": "INCREASE" }],
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let adjustment_id = body["data"]["id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["status"], "DRAFT");
        assert_eq!(body["data"]["version"], 1);

        let (status, body) = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/submit"),
                Some(&token),
                Some(json!({ "reviewed_by": "wh-reviewer-1" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "PENDING_WAREHOUSE_REVIEW");
        assert_eq!(body["data"]["reviewed_by"], "wh-reviewer-1");

        let (status, body) = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/approve"),
                Some(&token),
                Some(json!({ "finance_reviewed_by": "fin-reviewer-1" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "PENDING_FINANCE_REVIEW");

        let (status, body) = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "POSTED");

        // 过账不变量（§8.2 第 3 条）：流水 + 余额 + 调整单同时生效
        let balance = test_db
            .db()
            .stock_balances()
            .find_by_dimensions(
                &entities::ids::WarehouseId::new("wh-1"),
                &entities::ids::SkuId::new("sku-1"),
                &mut database::NoTransaction,
            )
            .await
            .unwrap()
            .expect("余额必须存在");
        assert_eq!(
            balance.on_hand_quantity.to_string(),
            "3.000000",
            "盘盈 3 后余额应为 3"
        );
        let movements = test_db
            .db()
            .stock_movements()
            .find_by_source_document(&adjustment_id, &mut database::NoTransaction)
            .await
            .unwrap();
        assert_eq!(movements.len(), 1);
        assert_eq!(movements[0].movement_type.as_str(), "STOCK_GAIN");
        assert_eq!(movements[0].quantity.to_string(), "3.000000");

        let (status, body) = api
            .get(&format!("/admin/stock-adjustments/{adjustment_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["adjustment"]["status"], "POSTED");
        assert_eq!(body["data"]["posted_movements"].as_array().unwrap().len(), 1);

        let (status, body) = api.get("/admin/stock-balances", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["on_hand_quantity"], "3.000000");

        let (status, body) = api.get("/admin/stock-movements", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["movement_type"], "STOCK_GAIN");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn adjustment_decrease_releases_reservation_then_deducts() {
    require_mongo!(async {
        let test_db = TestDb::new("inv_api_release").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_balance(test_db.db(), "bal-1", "wh-1", "sku-1", "10").await;

        use entities::ids::{SalesOrderLineId, StockReservationId};
        use entities::inventory::{ReservationStatus, StockReservation, StockReservationData};
        use entities::money::Quantity;
        use std::str::FromStr;
        let reservation = StockReservation::new(
            StockReservationId::new("rsv-1"),
            StockReservationData {
                warehouse_id: entities::ids::WarehouseId::new("wh-1"),
                sku_id: entities::ids::SkuId::new("sku-1"),
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
        test_db
            .db()
            .stock_reservations()
            .create(&reservation, &mut database::NoTransaction)
            .await
            .unwrap();
        test_db
            .db()
            .stock_balances()
            .reserve_quantity(
                "bal-1",
                Quantity::from_str("4").unwrap(),
                &mut database::NoTransaction,
            )
            .await
            .unwrap();

        let (status, body) = api
            .post(
                "/admin/stock-adjustments",
                Some(&token),
                Some(json!({
                    "adjustment_no": "ADJ-2026-002",
                    "warehouse_id": "wh-1",
                    "reason_type": "STOCK_LOSS",
                    "lines": [{ "sku_id": "sku-1", "quantity": "6", "direction": "DECREASE" }],
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let adjustment_id = body["data"]["id"].as_str().unwrap().to_string();
        let _ = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/submit"),
                Some(&token),
                Some(json!({ "reviewed_by": "wh-reviewer-1" })),
            )
            .await;
        let _ = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/approve"),
                Some(&token),
                Some(json!({ "finance_reviewed_by": "fin-reviewer-1" })),
            )
            .await;
        let (status, body) = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_ok_envelope(status, &body);

        // 预占 4 全额释放 + 扣减 6 → on_hand 4、reserved 0、available 4
        let balance = test_db
            .db()
            .stock_balances()
            .find_by_dimensions(
                &entities::ids::WarehouseId::new("wh-1"),
                &entities::ids::SkuId::new("sku-1"),
                &mut database::NoTransaction,
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(balance.on_hand_quantity.to_string(), "4.000000");
        assert_eq!(balance.reserved_quantity.to_string(), "0.000000");
        assert_eq!(balance.available_quantity.to_string(), "4.000000");
        let reservation = test_db
            .db()
            .stock_reservations()
            .find_by_id("rsv-1", &mut database::NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reservation.status.as_str(), "RELEASED");
    })
}

#[tokio::test]
#[ignore]
async fn adjustment_post_injection_failure_rolls_back_everything() {
    require_mongo!(async {
        let test_db = TestDb::new("inv_api_rollback").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/stock-adjustments",
                Some(&token),
                Some(json!({
                    "adjustment_no": "ADJ-2026-003",
                    "warehouse_id": "wh-1",
                    "reason_type": "STOCK_GAIN",
                    "lines": [{ "sku_id": "sku-1", "quantity": "3", "direction": "INCREASE" }],
                })),
            )
            .await;
        let adjustment_id = body["data"]["id"].as_str().unwrap().to_string();
        let _ = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/submit"),
                Some(&token),
                Some(json!({ "reviewed_by": "wh-reviewer-1" })),
            )
            .await;
        let _ = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/approve"),
                Some(&token),
                Some(json!({ "finance_reviewed_by": "fin-reviewer-1" })),
            )
            .await;
        // 注入失败：明细方向与原因类型矛盾（盘盈必增）
        let (status, body) = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(status, 400, "方向矛盾必须 400: {body}");

        // 全部不可见：状态未迁移、无流水、无余额
        let adjustment = test_db
            .db()
            .stock_adjustments()
            .find_by_id(&adjustment_id, &mut database::NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(adjustment.status.as_str(), "PENDING_FINANCE_REVIEW");
        let movements = test_db
            .db()
            .stock_movements()
            .find_by_source_document(&adjustment_id, &mut database::NoTransaction)
            .await
            .unwrap();
        assert!(movements.is_empty(), "注入失败后不得留下流水");
        let balance = test_db
            .db()
            .stock_balances()
            .find_by_dimensions(
                &entities::ids::WarehouseId::new("wh-1"),
                &entities::ids::SkuId::new("sku-1"),
                &mut database::NoTransaction,
            )
            .await
            .unwrap();
        assert!(balance.is_none(), "注入失败后不得留下余额");
    })
}

#[tokio::test]
#[ignore]
async fn repeated_adjustment_post_returns_409() {
    require_mongo!(async {
        let test_db = TestDb::new("inv_api_dup").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/stock-adjustments",
                Some(&token),
                Some(json!({
                    "adjustment_no": "ADJ-2026-004",
                    "warehouse_id": "wh-1",
                    "reason_type": "STOCK_GAIN",
                    "lines": [{ "sku_id": "sku-1", "quantity": "1", "direction": "INCREASE" }],
                })),
            )
            .await;
        let adjustment_id = body["data"]["id"].as_str().unwrap().to_string();
        let _ = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/submit"),
                Some(&token),
                Some(json!({ "reviewed_by": "wh-reviewer-1" })),
            )
            .await;
        let _ = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/approve"),
                Some(&token),
                Some(json!({ "finance_reviewed_by": "fin-reviewer-1" })),
            )
            .await;
        let (status, _) = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_ok_envelope(status, &body);

        let (status, body) = api
            .post(
                &format!("/admin/stock-adjustments/{adjustment_id}/post"),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(status, 409, "重复过账必须 409: {body}");
        assert_eq!(body["success"], false);

        // 只产生一条正式流水（唯一索引兜底）
        let movements = test_db
            .db()
            .stock_movements()
            .find_by_source_document(&adjustment_id, &mut database::NoTransaction)
            .await
            .unwrap();
        assert_eq!(movements.len(), 1);
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("inv_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/stock-balances", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("inv_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/stock-balances", Some(&token)).await;
        assert_eq!(status, 403, "无 stock_balance.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("inv_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/stock-adjustments",
                Some(&token),
                Some(json!({
                    "adjustment_no": "  ",
                    "warehouse_id": "wh-1",
                    "reason_type": "STOCK_GAIN",
                    "lines": [{ "sku_id": "sku-1", "quantity": "1", "direction": "INCREASE" }],
                })),
            )
            .await;
        assert_eq!(status, 400, "空白单号必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, _) = api
            .post(
                "/admin/stock-adjustments",
                Some(&token),
                Some(json!({ "adjustment_no": "ADJ-X" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/stock-adjustments",
                Some(&token),
                Some(json!({
                    "adjustment_no": "ADJ-X",
                    "warehouse_id": "wh-1",
                    "reason_type": "MARS",
                    "lines": []
                })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");

        let (status, body) = api
            .get("/admin/stock-balances?sort_by=quantity", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sorting_boundaries() {
    require_mongo!(async {
        let test_db = TestDb::new("inv_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        for index in 1..=3 {
            seed_balance(
                test_db.db(),
                &format!("bal-{index}"),
                "wh-1",
                &format!("sku-{index}"),
                "1",
            )
            .await;
        }

        let (status, body) = api
            .get(
                "/admin/stock-balances?page=2&page_size=1&sort_by=sku_id&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["page"], 2);
        assert_eq!(body["data"]["page_size"], 1);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["items"][0]["sku_id"], "sku-2");

        let (status, body) = api
            .get(
                "/admin/stock-balances?page=1&page_size=100&sort_by=created_at&sort_dir=desc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 3);

        let (status, body) = api
            .get("/admin/stock-balances?page=1&page_size=999", Some(&token))
            .await;
        assert_eq!(status, 400, "page_size 超界必须 400: {body}");
    })
}
