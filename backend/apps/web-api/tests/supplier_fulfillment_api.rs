//! 域 D32 `supplier_fulfillment` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test supplier_fulfillment_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号 + 本域直接 `p` 规则
//! （`grant_domain_permissions`，casbin `g(sub, sub)` 自反匹配）。
//! 跨域依赖数据（D25 连接/能力、D29 商城订单/明细、D24 供给修订、D30 售后申请）
//! 由本测试直接种子；供应商网关使用模拟网关，按连接地址配置注入失败路径，
//! 不发起真实网络请求。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use entities::common::time::Instant;
use entities::ids::{
    MallAfterSalesRequestId, MallAfterSalesRequestLineId, MallOrderId, MallOrderItemId, SupplierAccountId,
    SupplierApiCapabilityId, SupplierApiConnectionId, SupplierOfferingRevisionId,
};
use entities::mall_after_sales::{
    AfterSalesLineStatus, AfterSalesRequestType, MallAfterSalesRequest, MallAfterSalesRequestData,
    MallAfterSalesRequestLine, MallAfterSalesRequestLineData,
};
use entities::supplier_api::{
    ConnectionEnvironment, SupplierApiCapability, SupplierApiCapabilityCode, SupplierApiCapabilityData,
    SupplierApiCapabilityStatus, SupplierApiConnection, SupplierApiConnectionData,
    SupplierApiConnectionStatus,
};
use id_generator::next_id;
use mongodb::bson::{doc, to_document, Document};
use mongodb::Database;
use serde_json::{json, Value};
use std::str::FromStr;
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节，同时满足 config 校验与 test-support 签发要求）。
const TEST_JWT_SECRET: &str = "c11-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("supplier_fulfillment_order", "list"),
    ("supplier_fulfillment_order", "detail"),
    ("supplier_fulfillment_order", "submit"),
    ("supplier_fulfillment_order", "cancel"),
    ("supplier_fulfillment_order", "refund"),
    ("supplier_fulfillment_order", "reject"),
    ("supplier_refund_fact", "post"),
];
/// 默认模拟连接地址（接单成功）。
const DEFAULT_ENDPOINT: &str = "https://supplier.example.com/api";
/// 供应商与商城订单种子标识。
const SUPPLIER_ID: &str = "supplier-1";

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
upload_path = "c11-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c11-uploads-{}", uuid::Uuid::new_v4()));
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

/// 种子供应商连接（D25）并写入全部动作能力。
async fn seed_connection(db: &Database, id: &str, endpoint: &str) {
    let connection = SupplierApiConnection::new(
        SupplierApiConnectionId::new(id),
        SupplierApiConnectionData {
            supplier_id: SupplierAccountId::new(SUPPLIER_ID),
            connection_code: format!("CONN-{id}"),
            environment: ConnectionEnvironment::Production,
            endpoint_reference: endpoint.to_string(),
            credential_reference: None,
            rate_limit_policy: None,
            status: SupplierApiConnectionStatus::Active,
        },
        "seed",
    )
    .expect("连接种子构造失败");
    db.collection::<Document>(<mongodb::Database as database::SupplierApiExt>::SUPPLIER_API_CONNECTIONS)
        .insert_one(to_document(&connection).unwrap())
        .await
        .expect("插入连接种子失败");

    for code in [
        SupplierApiCapabilityCode::Order,
        SupplierApiCapabilityCode::Cancel,
        SupplierApiCapabilityCode::Refund,
    ] {
        let capability = SupplierApiCapability::new(
            SupplierApiCapabilityId::new(next_id()),
            SupplierApiCapabilityData {
                connection_id: SupplierApiConnectionId::new(id),
                capability_code: code,
                status: SupplierApiCapabilityStatus::Active,
                constraint_snapshot: None,
            },
        )
        .expect("能力种子构造失败");
        db.collection::<Document>(<mongodb::Database as database::SupplierApiExt>::SUPPLIER_API_CAPABILITIES)
            .insert_one(to_document(&capability).unwrap())
            .await
            .expect("插入能力种子失败");
    }
}

/// 种子商城订单与一条商城明细（D29 实体构造，保证可被跨域 Repository 反序列化）。
async fn seed_mall_order(db: &Database) -> (String, String) {
    let order_id = MallOrderId::new(next_id());
    let item_id = MallOrderItemId::new(next_id());
    let order = entities::mall_order::MallOrder::new(
        order_id.clone(),
        entities::mall_order::MallOrderData {
            mall_id: "mall-1".to_string(),
            external_order_no: "MO-2026-001".to_string(),
            payment_fact_id: entities::ids::MallOrderFactId::new(next_id()),
            mall_user_ref: "user-1".to_string(),
            source_customer_ref: None,
            customer_id: None,
            ordered_at: Instant::now(),
            paid_at: Instant::now(),
            gross_amount: entities::money::Amount::from_str("33.00").unwrap(),
            discount_amount: entities::money::Amount::from_str("0.00").unwrap(),
            freight_amount: entities::money::Amount::from_str("0.00").unwrap(),
            paid_amount: entities::money::Amount::from_str("33.00").unwrap(),
            fulfillment_chain: entities::mall_order::FulfillmentChain::ErpAutomated,
            attribution_status: entities::mall_order::AttributionStatus::Attributed,
            address_snapshot_encrypted: Some("encrypted".to_string()),
        },
    )
    .expect("商城订单种子构造失败");
    db.collection::<Document>(<mongodb::Database as database::MallOrderExt>::MALL_ORDERS)
        .insert_one(to_document(&order).unwrap())
        .await
        .expect("插入商城订单种子失败");
    let item = entities::mall_order::MallOrderItem::new(
        item_id.clone(),
        entities::mall_order::MallOrderItemData {
            mall_order_id: order_id.clone(),
            external_item_id: "MOL-1".to_string(),
            sku_id: None,
            product_publication_revision_id: None,
            supplier_offering_revision_id: Some(SupplierOfferingRevisionId::new("offering-rev-1")),
            name_snapshot: "测试商品".to_string(),
            spec_snapshot: None,
            quantity: entities::money::Quantity::from_str("3.000000").unwrap(),
            unit_price_gross: entities::money::UnitPrice::from_str("11.0000").unwrap(),
            line_gross_amount: entities::money::Amount::from_str("33.00").unwrap(),
            allocated_discount_amount: entities::money::Amount::from_str("0.00").unwrap(),
            allocated_freight_amount: entities::money::Amount::from_str("0.00").unwrap(),
            paid_amount: entities::money::Amount::from_str("33.00").unwrap(),
            sales_tax_rate: entities::money::Rate::from_str("0.130000").unwrap(),
            unit_cost_snapshot: None,
            cost_snapshot_total: None,
            cost_tax_inclusion: None,
            cost_input_tax_rate: None,
        },
    )
    .expect("商城订单明细种子构造失败");
    db.collection::<Document>(<mongodb::Database as database::MallOrderExt>::MALL_ORDER_ITEMS)
        .insert_one(to_document(&item).unwrap())
        .await
        .expect("插入商城订单明细种子失败");
    (order_id.to_string(), item_id.to_string())
}

/// 种子供应商供给修订（D24 实体构造，保证可被跨域 Repository 反序列化）。
async fn seed_offering_revision(db: &Database, id: &str) {
    let revision = entities::supplier_catalog::SupplierOfferingRevision::new(
        SupplierOfferingRevisionId::new(id),
        entities::supplier_catalog::SupplierOfferingRevisionData {
            supplier_offering_id: entities::ids::SupplierOfferingId::new(next_id()),
            revision_no: 1,
            dropship_supply_price_gross: entities::money::UnitPrice::from_str("9.9900").unwrap(),
            dropship_supply_price_net: entities::money::UnitPrice::from_str("8.6900").unwrap(),
            bulk_supply_price_gross: entities::money::UnitPrice::from_str("9.0000").unwrap(),
            bulk_supply_price_net: entities::money::UnitPrice::from_str("7.8300").unwrap(),
            input_tax_rate: entities::money::Rate::from_str("0.130000").unwrap(),
            dropship_express: None,
            freight_amount: None,
            service_fee_amount: None,
            bulk_minimum_order_quantity: entities::money::Quantity::from_str("1.000000").unwrap(),
            supply_region: vec!["全国".to_string()],
            availability_status: entities::supplier_catalog::AvailabilityStatus::Available,
            available_quantity: Some(entities::money::Quantity::from_str("100.000000").unwrap()),
            product_capabilities: vec![],
            valid_from: entities::common::time::BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: None,
            prefill_source_refs: entities::supplier_catalog::PrefillSourceRefs {
                input_tax_rate: None,
                supply_region: None,
                valid_from_date: None,
                valid_from_timezone: None,
                valid_from_calendar_version: None,
            },
        },
    )
    .expect("供给修订种子构造失败");
    db.collection::<Document>(
        <mongodb::Database as database::SupplierCatalogExt>::SUPPLIER_OFFERING_REVISIONS,
    )
    .insert_one(to_document(&revision).unwrap())
    .await
    .expect("插入供给修订种子失败");
}

/// 种子商城售后申请与一行申请行（D30，申请数量 3 / 金额 29.97）。
async fn seed_after_sales_request(db: &Database, mall_order_id: &str) -> (String, String) {
    let request_id = next_id();
    let line_id = next_id();
    let request = MallAfterSalesRequest::new(
        MallAfterSalesRequestId::new(&request_id),
        MallAfterSalesRequestData {
            mall_id: "mall-1".to_string(),
            external_request_id: "ASR-2026-001".to_string(),
            mall_order_id: MallOrderId::new(mall_order_id),
            request_type: AfterSalesRequestType::Refund,
            reason: "质量问题".to_string(),
            created_at: Instant::now(),
        },
    )
    .expect("售后申请种子构造失败");
    db.collection::<Document>(<mongodb::Database as database::MallAfterSalesExt>::MALL_AFTER_SALES_REQUESTS)
        .insert_one(to_document(&request).unwrap())
        .await
        .expect("插入售后申请种子失败");

    let line = MallAfterSalesRequestLine::new(
        MallAfterSalesRequestLineId::new(&line_id),
        MallAfterSalesRequestLineData {
            after_sales_request_id: MallAfterSalesRequestId::new(&request_id),
            line_no: 1,
            mall_order_item_id: MallOrderItemId::new("mall-item-placeholder"),
            supplier_fulfillment_item_id: None,
            requested_quantity: entities::money::Quantity::from_str("3.000000").unwrap(),
            requested_amount: entities::money::Amount::from_str("29.97").unwrap(),
            line_status: AfterSalesLineStatus::Pending,
        },
    )
    .expect("售后申请行种子构造失败");
    db.collection::<Document>(
        <mongodb::Database as database::MallAfterSalesExt>::MALL_AFTER_SALES_REQUEST_LINES,
    )
    .insert_one(to_document(&line).unwrap())
    .await
    .expect("插入售后申请行种子失败");
    (request_id, line_id)
}

/// 构造下单请求体（1 行明细：单价 9.99 × 3 = 29.97）。
fn place_body(connection_id: &str, mall_order_id: &str, mall_item_id: &str, order_no: &str) -> Value {
    json!({
        "fulfillment_order_no": order_no,
        "mall_order_id": mall_order_id,
        "supplier_id": SUPPLIER_ID,
        "connection_id": connection_id,
        "split_no": 1,
        "address_snapshot_encrypted": "encrypted-address",
        "address_snapshot_fingerprint": "fingerprint-address",
        "items": [{
            "mall_order_item_id": mall_item_id,
            "supplier_offering_revision_id": "offering-rev-1",
            "supplier_catalog_sku_id": "catalog-sku-1",
            "quantity": "3.000000",
            "unit_cost_snapshot_gross": "9.9900",
            "input_tax_rate": "0.130000"
        }]
    })
}

/// 提交下单并断言成功，返回订单视图。
async fn place_order(api: &TestApi, token: &str, body: Value) -> Value {
    let (status, body) = api
        .post("/admin/supplier-fulfillment-orders", Some(token), Some(body))
        .await;
    assert_ok_envelope(status, &body);
    body["data"].clone()
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/supplier-fulfillment-orders", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/supplier-fulfillment-orders", Some(&token)).await;
        assert_eq!(
            status, 403,
            "无 supplier_fulfillment_order.list 权限必须 403: {body}"
        );
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let mut body = place_body("conn-1", "mall-order-1", "mall-item-1", "FO-400");
        body["fulfillment_order_no"] = json!("   ");
        let (status, body) = api
            .post("/admin/supplier-fulfillment-orders", Some(&token), Some(body))
            .await;
        assert_eq!(status, 400, "空白订单号必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null, "失败时 data 恒为 null");

        let (status, _) = api
            .post(
                "/admin/supplier-fulfillment-orders",
                Some(&token),
                Some(json!({ "fulfillment_order_no": "FO-400" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_place_then_detail_and_list_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_connection(test_db.db(), "conn-1", DEFAULT_ENDPOINT).await;
        let (mall_order_id, mall_item_id) = seed_mall_order(test_db.db()).await;
        seed_offering_revision(test_db.db(), "offering-rev-1").await;

        let created = place_order(
            &api,
            &token,
            place_body("conn-1", &mall_order_id, &mall_item_id, "FO-2026-001"),
        )
        .await;
        assert_eq!(created["fulfillment_order_no"], "FO-2026-001");
        assert_eq!(created["fulfillment_status"], "ACCEPTED", "模拟网关接单成功");
        assert_eq!(created["external_order_no"], "EXT-FO-2026-001");
        assert_eq!(created["version"], 2, "下单事务 + 派发结果写回各递增一次");
        assert_eq!(created["mall_order_id"], mall_order_id);
        assert!(created["submitted_at"].as_i64().is_some());
        assert!(created["accepted_at"].as_i64().is_some());

        let order_id = created["id"].as_str().unwrap();
        let (status, body) = api
            .get(
                &format!("/admin/supplier-fulfillment-orders/{order_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["order"]["fulfillment_status"], "ACCEPTED");
        assert_eq!(detail["items"].as_array().unwrap().len(), 1);
        let item = &detail["items"][0];
        assert_eq!(item["quantity"], "3.000000");
        assert_eq!(item["cost_snapshot_total_gross"], "29.97", "成本快照按分舍入");
        assert_eq!(detail["actions"].as_array().unwrap().len(), 1);
        assert_eq!(detail["actions"][0]["action_type"], "PLACE");
        assert_eq!(detail["actions"][0]["status"], "SUCCEEDED");
        assert_eq!(detail["status_history"].as_array().unwrap().len(), 0);

        let (status, body) = api
            .get(
                "/admin/supplier-fulfillment-orders?fulfillment_status=ACCEPTED&page_size=1",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 1);
        let row = &data["items"][0];
        for field in [
            "id",
            "fulfillment_order_no",
            "mall_order_id",
            "supplier_id",
            "connection_id",
            "fulfillment_status",
            "cancel_status",
            "refund_status",
            "version",
            "created_at",
        ] {
            assert!(row.get(field).is_some(), "契约字段 {field} 必须存在: {row}");
        }

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn place_is_idempotent_by_fulfillment_order_no() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_idem_place").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_connection(test_db.db(), "conn-1", DEFAULT_ENDPOINT).await;
        let (mall_order_id, mall_item_id) = seed_mall_order(test_db.db()).await;
        seed_offering_revision(test_db.db(), "offering-rev-1").await;

        let first = place_order(
            &api,
            &token,
            place_body("conn-1", &mall_order_id, &mall_item_id, "FO-IDEM"),
        )
        .await;
        let second = place_order(
            &api,
            &token,
            place_body("conn-1", &mall_order_id, &mall_item_id, "FO-IDEM"),
        )
        .await;
        assert_eq!(first["id"], second["id"], "重复下单必须返回原订单");
        assert_eq!(first["version"], second["version"], "幂等命中不得推进版本");

        let action_count = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTIONS,
            )
            .count_documents(doc! {})
            .await
            .expect("统计动作失败");
        assert_eq!(action_count, 1, "重复下单只产生一条 PLACE 动作");
    })
}

#[tokio::test]
#[ignore]
async fn reject_records_history_and_is_idempotent() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_reject").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_connection(test_db.db(), "conn-1", DEFAULT_ENDPOINT).await;
        let (mall_order_id, mall_item_id) = seed_mall_order(test_db.db()).await;
        seed_offering_revision(test_db.db(), "offering-rev-1").await;
        let order_created = place_order(
            &api,
            &token,
            place_body("conn-1", &mall_order_id, &mall_item_id, "FO-REJ"),
        )
        .await;
        let order_id = order_created["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/supplier-fulfillment-orders/{order_id}/reject"),
                Some(&token),
                Some(json!({
                    "external_event_id": "EVT-REJ-1",
                    "supplier_status_version": "v2",
                    "occurred_at": 1753000000,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["new_status"], "REJECTED");
        assert_eq!(body["data"]["external_event_id"], "EVT-REJ-1");

        let (_, detail) = api
            .get(
                &format!("/admin/supplier-fulfillment-orders/{order_id}"),
                Some(&token),
            )
            .await;
        assert_eq!(detail["data"]["order"]["fulfillment_status"], "REJECTED");
        assert_eq!(
            detail["data"]["actions"][0]["status"], "FAILED",
            "拒单同时标记 PLACE 动作失败"
        );
        assert_eq!(detail["data"]["status_history"].as_array().unwrap().len(), 1);

        let (status, body) = api
            .post(
                &format!("/admin/supplier-fulfillment-orders/{order_id}/reject"),
                Some(&token),
                Some(json!({
                    "external_event_id": "EVT-REJ-1",
                    "supplier_status_version": "v2",
                    "occurred_at": 1753000000,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let history_count = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_ORDER_STATUS_HISTORIES,
            )
            .count_documents(doc! {})
            .await
            .expect("统计状态历史失败");
        assert_eq!(history_count, 1, "重复拒单回调只产生一条状态历史");
    })
}

#[tokio::test]
#[ignore]
async fn reject_transaction_invariant_rolls_back_on_injected_failure() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_reject_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_connection(test_db.db(), "conn-1", DEFAULT_ENDPOINT).await;
        let (mall_order_id, mall_item_id) = seed_mall_order(test_db.db()).await;
        seed_offering_revision(test_db.db(), "offering-rev-1").await;
        let order_created = place_order(
            &api,
            &token,
            place_body("conn-1", &mall_order_id, &mall_item_id, "FO-REJTX"),
        )
        .await;
        let order_id = order_created["id"].as_str().unwrap().to_string();

        // 注入失败：预置同 (connection_id, external_event_id) 状态历史，
        // 事务内唯一索引冲突使「订单状态 + 状态历史 + 动作结果」整体回滚。
        test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_ORDER_STATUS_HISTORIES,
            )
            .insert_one(doc! {
                "id": next_id(),
                "version": 1, "created_at": 0, "updated_at": 0, "deleted_at": 0,
                "connection_id": "conn-1",
                "previous_status": "SUBMITTING",
                "new_status": "REJECTED",
                "supplier_status_version": "v2",
                "occurred_at": 1753000000,
                "received_at": 1753000001,
                "external_event_id": "EVT-REJTX",
                "source_type": "supplier_callback",
            })
            .await
            .expect("预置冲突状态历史失败");

        let (status, _) = api
            .post(
                &format!("/admin/supplier-fulfillment-orders/{order_id}/reject"),
                Some(&token),
                Some(json!({
                    "external_event_id": "EVT-REJTX",
                    "supplier_status_version": "v2",
                    "occurred_at": 1753000000,
                })),
            )
            .await;
        assert_eq!(status, 409, "回调事件冲突必须 409");

        let (_, detail) = api
            .get(
                &format!("/admin/supplier-fulfillment-orders/{order_id}"),
                Some(&token),
            )
            .await;
        assert_eq!(
            detail["data"]["order"]["fulfillment_status"], "ACCEPTED",
            "注入失败后订单状态必须保持原状"
        );
        assert_eq!(
            detail["data"]["actions"][0]["status"], "SUCCEEDED",
            "注入失败后 PLACE 动作不得被改写"
        );
        let history_count = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_ORDER_STATUS_HISTORIES,
            )
            .count_documents(doc! {})
            .await
            .expect("统计状态历史失败");
        assert_eq!(history_count, 1, "注入失败后不产生新的状态历史");
    })
}

#[tokio::test]
#[ignore]
async fn cancel_submits_advances_status_and_is_idempotent() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_cancel").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_connection(test_db.db(), "conn-1", DEFAULT_ENDPOINT).await;
        let (mall_order_id, mall_item_id) = seed_mall_order(test_db.db()).await;
        seed_offering_revision(test_db.db(), "offering-rev-1").await;
        let (after_sales_request_id, after_sales_line_id) =
            seed_after_sales_request(test_db.db(), &mall_order_id).await;
        let order_created = place_order(
            &api,
            &token,
            place_body("conn-1", &mall_order_id, &mall_item_id, "FO-CXL"),
        )
        .await;
        let order_id = order_created["id"].as_str().unwrap().to_string();
        let fulfillment_item_id = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
            )
            .find_one(doc! { "supplier_fulfillment_order_id": &order_id })
            .await
            .expect("查询履约明细失败")
            .expect("履约明细必须存在")["id"]
            .as_str()
            .unwrap()
            .to_string();

        let cancel_body = json!({
            "after_sales_request_id": after_sales_request_id,
            "lines": [{
                "after_sales_request_line_id": after_sales_line_id,
                "supplier_fulfillment_item_id": fulfillment_item_id,
                "quantity": "2.000000",
                "amount": "19.98"
            }],
            "reason_code": "QUALITY_ISSUE"
        });
        let (status, body) = api
            .post(
                &format!("/admin/supplier-fulfillment-orders/{order_id}/cancel"),
                Some(&token),
                Some(cancel_body.clone()),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["action"]["action_type"], "CANCEL");
        assert_eq!(body["data"]["action"]["status"], "SUCCEEDED");
        assert_eq!(body["data"]["order"]["cancel_status"], "CANCEL_PENDING");

        let (status, body) = api
            .post(
                &format!("/admin/supplier-fulfillment-orders/{order_id}/cancel"),
                Some(&token),
                Some(cancel_body),
            )
            .await;
        assert_ok_envelope(status, &body);
        let action_count = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTIONS,
            )
            .count_documents(doc! { "action_type": "CANCEL" })
            .await
            .expect("统计动作失败");
        assert_eq!(action_count, 1, "重复取消只产生一条 CANCEL 动作");

        let (status, _) = api
            .post(
                &format!("/admin/supplier-fulfillment-orders/{order_id}/cancel"),
                Some(&token),
                Some(json!({
                    "after_sales_request_id": after_sales_request_id,
                    "lines": [{
                        "after_sales_request_line_id": after_sales_line_id,
                        "supplier_fulfillment_item_id": fulfillment_item_id,
                        "quantity": "3.000000",
                        "amount": "29.97"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 422, "超过申请行尚未提交的净余额必须 422");
    })
}

#[tokio::test]
#[ignore]
async fn refund_result_writes_fact_and_allocations_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_refund").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_connection(test_db.db(), "conn-1", DEFAULT_ENDPOINT).await;
        let (mall_order_id, mall_item_id) = seed_mall_order(test_db.db()).await;
        seed_offering_revision(test_db.db(), "offering-rev-1").await;
        let (after_sales_request_id, after_sales_line_id) =
            seed_after_sales_request(test_db.db(), &mall_order_id).await;
        let order_created = place_order(
            &api,
            &token,
            place_body("conn-1", &mall_order_id, &mall_item_id, "FO-RFD"),
        )
        .await;
        let order_id = order_created["id"].as_str().unwrap().to_string();
        let fulfillment_item_id = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
            )
            .find_one(doc! { "supplier_fulfillment_order_id": &order_id })
            .await
            .expect("查询履约明细失败")
            .expect("履约明细必须存在")["id"]
            .as_str()
            .unwrap()
            .to_string();

        let (status, _) = api
            .post(
                &format!("/admin/supplier-fulfillment-orders/{order_id}/refund"),
                Some(&token),
                Some(json!({
                    "after_sales_request_id": after_sales_request_id,
                    "lines": [{
                        "after_sales_request_line_id": after_sales_line_id,
                        "supplier_fulfillment_item_id": fulfillment_item_id,
                        "quantity": "3.000000",
                        "amount": "29.97"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &json!({}));

        let refund_body = json!({
            "external_refund_no": "REF-1001",
            "external_refund_version": "1",
            "refund_amount": "29.97",
            "refunded_at": 1753000000,
            "source_event_id": "EVT-REFUND-1",
            "allocations": [{
                "supplier_fulfillment_item_id": fulfillment_item_id,
                "original_cost_entry_id": "cost-entry-1",
                "original_cost_allocation_id": "cost-allocation-1",
                "original_payable_entry_id": "payable-entry-1",
                "original_payment_allocation_id": "payment-alloc-1",
                "refund_quantity": "3.000000",
                "gross_amount": "29.97",
                "net_amount": "26.07",
                "tax_amount": "3.90",
                "payable_reduction_amount": "14.99",
                "cash_refund_amount": "14.98"
            }]
        });
        let (status, body) = api
            .post(
                "/admin/supplier-refund-facts",
                Some(&token),
                Some(refund_body.clone()),
            )
            .await;
        assert_ok_envelope(status, &body);
        let fact = &body["data"];
        assert_eq!(fact["external_refund_no"], "REF-1001");
        assert_eq!(fact["refund_amount"], "29.97");
        assert_eq!(fact["allocations"].as_array().unwrap().len(), 1);

        let (_, detail) = api
            .get(
                &format!("/admin/supplier-fulfillment-orders/{order_id}"),
                Some(&token),
            )
            .await;
        assert_eq!(
            detail["data"]["order"]["refund_status"], "REFUNDED",
            "累计退款等于订单成本余额时进入 REFUNDED"
        );
        assert_eq!(detail["data"]["refund_facts"].as_array().unwrap().len(), 1);

        let (status, body) = api
            .post("/admin/supplier-refund-facts", Some(&token), Some(refund_body))
            .await;
        assert_ok_envelope(status, &body);
        let fact_count = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
            )
            .count_documents(doc! {})
            .await
            .expect("统计退款事实失败");
        assert_eq!(fact_count, 1, "重复退款结果只产生一条正式事实");

        let (status, _) = api
            .post(
                "/admin/supplier-refund-facts",
                Some(&token),
                Some(json!({
                    "external_refund_no": "REF-2001",
                    "external_refund_version": "1",
                    "refund_amount": "10.00",
                    "refunded_at": 1753000000,
                    "source_event_id": "EVT-REFUND-2",
                    "allocations": [{
                        "supplier_fulfillment_item_id": fulfillment_item_id,
                        "original_cost_entry_id": "cost-entry-1",
                        "original_cost_allocation_id": "cost-allocation-1",
                        "original_payable_entry_id": "payable-entry-1",
                        "original_payment_allocation_id": "payment-alloc-1",
                        "refund_quantity": "1.000000",
                        "gross_amount": "10.00",
                        "net_amount": "8.70",
                        "tax_amount": "1.30",
                        "payable_reduction_amount": "5.00",
                        "cash_refund_amount": "5.00"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 422, "累计退款超过订单成本余额必须 422");
    })
}

#[tokio::test]
#[ignore]
async fn refund_result_transaction_invariant_rolls_back_on_injected_failure() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_refund_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_connection(test_db.db(), "conn-1", DEFAULT_ENDPOINT).await;
        let (mall_order_id, mall_item_id) = seed_mall_order(test_db.db()).await;
        seed_offering_revision(test_db.db(), "offering-rev-1").await;
        let (after_sales_request_id, after_sales_line_id) =
            seed_after_sales_request(test_db.db(), &mall_order_id).await;
        let order_created = place_order(
            &api,
            &token,
            place_body("conn-1", &mall_order_id, &mall_item_id, "FO-RFDTX"),
        )
        .await;
        let order_id = order_created["id"].as_str().unwrap().to_string();
        let fulfillment_item_id = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS,
            )
            .find_one(doc! { "supplier_fulfillment_order_id": &order_id })
            .await
            .expect("查询履约明细失败")
            .expect("履约明细必须存在")["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (status, _) = api
            .post(
                &format!("/admin/supplier-fulfillment-orders/{order_id}/refund"),
                Some(&token),
                Some(json!({
                    "after_sales_request_id": after_sales_request_id,
                    "lines": [{
                        "after_sales_request_line_id": after_sales_line_id,
                        "supplier_fulfillment_item_id": fulfillment_item_id,
                        "quantity": "3.000000",
                        "amount": "29.97"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &json!({}));

        // 注入失败：预置同 (connection, refund_no, version) 退款事实头，
        // 事务内唯一索引冲突使「订单状态 + 退款头 + 分配行」整体回滚。
        test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS,
            )
            .insert_one(doc! {
                "id": next_id(),
                "version": 1, "created_at": 0, "updated_at": 0, "deleted_at": 0,
                "supplier_id": SUPPLIER_ID,
                "connection_id": "conn-1",
                "supplier_fulfillment_order_id": &order_id,
                "external_refund_no": "REF-TX-1",
                "external_refund_version": "1",
                "refund_amount": "29.97",
                "refunded_at": 1753000000,
                "source_event_id": "EVT-TX",
                "inbox_message_id": next_id(),
            })
            .await
            .expect("预置冲突退款事实失败");

        let (status, _) = api
            .post(
                "/admin/supplier-refund-facts",
                Some(&token),
                Some(json!({
                    "external_refund_no": "REF-TX-1",
                    "external_refund_version": "1",
                    "refund_amount": "29.97",
                    "refunded_at": 1753000000,
                    "source_event_id": "EVT-TX",
                    "allocations": [{
                        "supplier_fulfillment_item_id": fulfillment_item_id,
                        "original_cost_entry_id": "cost-entry-1",
                        "original_cost_allocation_id": "cost-allocation-1",
                        "original_payable_entry_id": "payable-entry-1",
                        "original_payment_allocation_id": "payment-alloc-1",
                        "refund_quantity": "3.000000",
                        "gross_amount": "29.97",
                        "net_amount": "26.07",
                        "tax_amount": "3.90",
                        "payable_reduction_amount": "14.99",
                        "cash_refund_amount": "14.98"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 409, "外部退款身份冲突必须 409");

        let (_, detail) = api
            .get(
                &format!("/admin/supplier-fulfillment-orders/{order_id}"),
                Some(&token),
            )
            .await;
        assert_eq!(
            detail["data"]["order"]["refund_status"], "REFUND_PENDING",
            "注入失败后退款进度必须保持原状"
        );
        let allocation_count = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS,
            )
            .count_documents(doc! {})
            .await
            .expect("统计分配行失败");
        assert_eq!(allocation_count, 0, "注入失败后不得留下分配行");
    })
}

#[tokio::test]
#[ignore]
async fn dispatch_failure_degrades_to_integration_error_task() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_fail").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_connection(test_db.db(), "conn-1", "sim://temporary-failure").await;
        let (mall_order_id, mall_item_id) = seed_mall_order(test_db.db()).await;
        seed_offering_revision(test_db.db(), "offering-rev-1").await;

        let created = place_order(
            &api,
            &token,
            place_body("conn-1", &mall_order_id, &mall_item_id, "FO-FAIL"),
        )
        .await;
        assert_eq!(
            created["fulfillment_status"], "SUBMITTING",
            "临时故障保持提交中等待自动重试"
        );

        let error_task = test_db
            .db()
            .collection::<Document>(
                <mongodb::Database as database::IntegrationOpsExt>::INTEGRATION_ERROR_TASKS,
            )
            .find_one(doc! {})
            .await
            .expect("查询错误任务失败")
            .expect("必须生成集成错误任务");
        assert_eq!(error_task.get_str("error_class").unwrap(), "transient_failure");
        assert_eq!(error_task.get_str("status").unwrap(), "pending");

        let inbox = test_db
            .db()
            .collection::<Document>(<mongodb::Database as database::IntegrationOpsExt>::INBOX_MESSAGES)
            .find_one(doc! {})
            .await
            .expect("查询 inbox 消息失败")
            .expect("必须生成 inbox 消息");
        assert_eq!(
            inbox.get_str("status").unwrap(),
            "failed",
            "失败消息标记为 failed"
        );

        let (_, detail) = api
            .get(
                &format!(
                    "/admin/supplier-fulfillment-orders/{}",
                    created["id"].as_str().unwrap()
                ),
                Some(&token),
            )
            .await;
        assert_eq!(
            detail["data"]["order"]["external_order_no"],
            Value::Null,
            "失败不得回写外部单号"
        );
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_boundaries_are_enforced() {
    require_mongo!(async {
        let test_db = TestDb::new("sf_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        seed_connection(test_db.db(), "conn-1", DEFAULT_ENDPOINT).await;
        let (mall_order_id, mall_item_id) = seed_mall_order(test_db.db()).await;
        seed_offering_revision(test_db.db(), "offering-rev-1").await;
        for order_no in ["FO-P1", "FO-P2", "FO-P3"] {
            place_order(
                &api,
                &token,
                place_body("conn-1", &mall_order_id, &mall_item_id, order_no),
            )
            .await;
        }

        let (status, body) = api
            .get(
                "/admin/supplier-fulfillment-orders?page_size=1&page=2&sort_by=submitted_at&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["page"], 2);

        let (status, body) = api
            .get(
                "/admin/supplier-fulfillment-orders?sort_by=fulfillment_status",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");

        let (status, _) = api
            .get("/admin/supplier-fulfillment-orders?page_size=0", Some(&token))
            .await;
        assert_eq!(status, 400, "非法分页大小必须 400");
    })
}
