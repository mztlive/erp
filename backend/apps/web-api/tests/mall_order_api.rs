//! 域 D29 `mall_order` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控；种子账号 + 本域直接 `p` 规则
//! 构造 403 用例。覆盖：消费入账事务不变量（§8.4 第 3、7 条）、幂等、
//! 守恒失败全量不可见、取消/完成事实、分页与排序边界。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use entities::card_instance::{
    CardSourceType, MallCardInstance, MallCardInstanceData, MallConsumptionCutover,
    MallConsumptionCutoverData,
};
use entities::common::time::Instant;
use entities::ids::{
    ExternalIdentityMapId, MallCardInstanceId, MallConsumptionCutoverId, SalesOrderId, SalesOrderRevisionId,
};
use entities::money::Amount;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use std::str::FromStr;
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节）。
const TEST_JWT_SECRET: &str = "c-g10-mall-order-test-secret-32-bytes-min";
/// 种子账号可访问的本域权限键。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("mall_order", "list"),
    ("mall_order", "detail"),
    ("mall_order_fact", "list"),
    ("mall_order_fact", "submit"),
];

/// 为种子账号插入本域直接 `p` 规则。
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

/// 构造最小 AppState 并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "c-g10-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c-g10-uploads-{}", uuid::Uuid::new_v4()));
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

/// 种子卡实例基线（直接写库，消费归集按 `(mall_id, ref)` 匹配）。
async fn seed_card_instance(db: &Database) {
    let instance = MallCardInstance::new(
        MallCardInstanceId::new("card-inst-1"),
        MallCardInstanceData {
            mall_id: "mall-a".to_string(),
            opaque_instance_ref: "card-ref-001".to_string(),
            origin_sales_order_source_identity_id: ExternalIdentityMapId::new("ext-map-1"),
            origin_sales_order_id: SalesOrderId::new("so-1"),
            origin_sales_order_revision_id: SalesOrderRevisionId::new("so-rev-1"),
            source_baseline_version: None,
            initial_balance: Amount::from_str("500.00").unwrap(),
            baseline_at: Instant::from_unix_secs(1_749_000_000),
            source_type: CardSourceType::Realtime,
        },
    )
    .expect("卡实例实体构造失败");
    db.collection::<MallCardInstance>("mall_card_instances")
        .insert_one(&instance)
        .await
        .expect("卡实例种子写入失败");
}

/// 种子已启用的切换记录（`T` 起进入 ERP_AUTOMATED 履约链）。
async fn seed_enabled_cutover(db: &Database, t: i64) {
    let mut cutover = MallConsumptionCutover::new(
        MallConsumptionCutoverId::new("cutover-1"),
        MallConsumptionCutoverData {
            mall_id: "mall-a".to_string(),
            checklist_reference: None,
        },
    )
    .expect("切换实体构造失败");
    cutover
        .enable(Instant::from_unix_secs(t), "tester")
        .expect("启用切换失败");
    db.collection::<MallConsumptionCutover>("mall_consumption_cutovers")
        .insert_one(&cutover)
        .await
        .expect("切换种子写入失败");
}

/// 构造标准支付事实载荷（1 商品 × 卡券 30 + 微信 20，成本含税 40 @ 13%）。
fn payment_payload(suffix: &str) -> Value {
    json!({
        "mall_id": "mall-a",
        "source_event_id": format!("evt-pay-{suffix}"),
        "inbox_message_id": format!("inbox-pay-{suffix}"),
        "business_fact_key": format!("mall-a:PAYMENT:SO-{suffix}:v1"),
        "fact_type": "PAYMENT_SUCCEEDED",
        "external_order_no": format!("SO-{suffix}"),
        "external_order_version": "v1",
        "occurred_at": 1_750_000_000,
        "received_at": 1_750_000_010,
        "data_source": "realtime",
        "payment": {
            "mall_user_ref": "user-1",
            "source_customer_ref": "cust-ref-1",
            "ordered_at": 1_749_999_900,
            "gross_amount": "50.00",
            "discount_amount": "0.00",
            "freight_amount": "0.00",
            "paid_amount": "50.00",
            "items": [{
                "external_item_id": "item-1",
                "name_snapshot": "测试商品",
                "spec_snapshot": "标准",
                "quantity": "1.000000",
                "unit_price_gross": "50.0000",
                "allocated_discount_amount": "0.00",
                "allocated_freight_amount": "0.00",
                "sales_tax_rate": "0.130000",
                "cost_snapshot_total": "40.00",
                "cost_tax_inclusion": true,
                "cost_input_tax_rate": "0.130000"
            }],
            "payment_sources": [
                { "source_no": 1, "source_type": "CARD", "amount": "30.00", "source_card_instance_ref": "card-ref-001" },
                { "source_no": 2, "source_type": "WECHAT", "amount": "20.00", "wechat_payment_ref": "wx-1" }
            ],
            "funding_allocations": [
                { "external_item_id": "item-1", "source_no": 1, "allocated_payment_amount": "30.00" },
                { "external_item_id": "item-1", "source_no": 2, "allocated_payment_amount": "20.00" }
            ]
        }
    })
}

#[tokio::test]
#[ignore]
async fn payment_fact_receive_writes_consumption_and_cost_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_order_api_pay").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_card_instance(test_db.db()).await;
        seed_enabled_cutover(test_db.db(), 1_700_000_000).await;

        let (status, body) = api
            .post(
                "/admin/mall-order-facts",
                Some(&token),
                Some(payment_payload("1")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let received = &body["data"];
        assert_eq!(received["fact_type"], "PAYMENT_SUCCEEDED");
        assert_eq!(received["idempotent_hit"], false);
        let order_id = received["mall_order_id"].as_str().unwrap().to_string();
        let fact_id = received["fact"]["fact_id"].as_str().unwrap().to_string();

        // §8.4 第 3、7 条：事实 + 订单 + 消费 + 成本同事务生效。
        let facts = test_db
            .db()
            .collection::<Document>("mall_order_facts")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(facts, 1, "只有一条正式支付事实");
        let orders = test_db
            .db()
            .collection::<Document>("mall_orders")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(orders, 1, "只有一份唯一订单");
        let entries = test_db
            .db()
            .collection::<Document>("mall_consumption_entries")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(entries, 2, "两条消费事实（卡券 + 微信）");
        let assessments = test_db
            .db()
            .collection::<Document>("mall_consumption_cost_assessments")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(assessments, 2, "每条消费一条成本评估");
        let cost_entries = test_db
            .db()
            .collection::<Document>("cost_entries")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(cost_entries, 2, "ACTUAL 成本事实（商城消费 + 微信成本）");

        // 详情契约形状。
        let (status, body) = api
            .get(&format!("/admin/mall-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["identity"]["mall_order_id"], order_id);
        assert_eq!(detail["identity"]["external_order_no"], "SO-1");
        assert_eq!(detail["identity"]["payment_fact_id"], fact_id);
        assert_eq!(detail["fulfillment"]["chain"], "ERP_AUTOMATED");
        assert_eq!(detail["amounts"]["conservation_status"], "VALID");
        assert_eq!(detail["conservation"]["order_total"]["valid"], true);
        assert_eq!(detail["items"][0]["name_snapshot"], "测试商品");
        assert_eq!(detail["payment_sources"].as_array().unwrap().len(), 2);
        assert_eq!(detail["funding_allocations"].as_array().unwrap().len(), 2);
        assert_eq!(detail["consumption_entries"].as_array().unwrap().len(), 2);
        assert_eq!(detail["facts"].as_array().unwrap().len(), 1);
        let entry = &detail["consumption_entries"][0];
        assert_eq!(entry["current_cost_assessment"]["cost_basis"], "ACTUAL");
        assert!(entry["current_cost_assessment"]["gross_amount"].is_string());
        assert_eq!(entry["direction"], "consumption");

        // 列表契约形状。
        let (status, body) = api.get("/admin/mall-orders?mall_id=mall-a", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let row = &body["data"]["items"][0];
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(row["mall_order_id"], order_id);
        assert_eq!(row["paid_amount"], "50.00");
        assert_eq!(row["payment_composition"]["card_amount"], "30.00");
        assert_eq!(row["payment_composition"]["wechat_amount"], "20.00");
        assert_eq!(row["payment_composition"]["source_count"], 2);
        assert_eq!(row["fulfillment_chain"], "ERP_AUTOMATED");
        assert_eq!(row["attribution_status"], "attributed");
        assert_eq!(row["normalized_cost_basis"], "ACTUAL");
        assert_eq!(row["fact_summary"][0]["fact_type"], "PAYMENT_SUCCEEDED");
        assert_eq!(row["fact_summary"][0]["count"], 1);
    })
}

#[tokio::test]
#[ignore]
async fn payment_fact_receive_is_idempotent_by_business_fact_key() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_order_api_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_card_instance(test_db.db()).await;

        let (status, body) = api
            .post(
                "/admin/mall-order-facts",
                Some(&token),
                Some(payment_payload("dup")),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["idempotent_hit"], false);

        let (status, body) = api
            .post(
                "/admin/mall-order-facts",
                Some(&token),
                Some(payment_payload("dup")),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["idempotent_hit"], true, "重复提交幂等命中");

        let facts = test_db
            .db()
            .collection::<Document>("mall_order_facts")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(facts, 1, "重复提交只产生一条正式事实");
        let orders = test_db
            .db()
            .collection::<Document>("mall_orders")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(orders, 1, "重复提交不产生第二份订单");
        let entries = test_db
            .db()
            .collection::<Document>("mall_consumption_entries")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(entries, 2, "重复提交不重复消费");
    })
}

#[tokio::test]
#[ignore]
async fn conservation_failure_rolls_back_whole_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_order_api_cons").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_card_instance(test_db.db()).await;

        // 分摊合计 40 ≠ 实付 50：行/列守恒不成立。
        let mut payload = payment_payload("bad");
        payload["payment"]["funding_allocations"][1]["allocated_payment_amount"] = json!("10.00");
        let (status, body) = api
            .post("/admin/mall-order-facts", Some(&token), Some(payload))
            .await;
        assert_eq!(status, 422, "守恒失败必须 422: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 注入失败全部不可见：无事实、无订单、无消费、无成本。
        for collection in [
            "mall_order_facts",
            "mall_orders",
            "mall_consumption_entries",
            "mall_consumption_cost_assessments",
            "cost_entries",
        ] {
            let count = test_db
                .db()
                .collection::<Document>(collection)
                .count_documents(doc! {})
                .await
                .unwrap();
            assert_eq!(count, 0, "集合 {collection} 必须无残留");
        }
    })
}

#[tokio::test]
#[ignore]
async fn cancel_and_completion_facts_attach_to_original_payment() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_order_api_cancel").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_card_instance(test_db.db()).await;

        let (_, body) = api
            .post(
                "/admin/mall-order-facts",
                Some(&token),
                Some(payment_payload("1")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let payment_fact_id = body["data"]["fact"]["fact_id"].as_str().unwrap().to_string();
        let order_id = body["data"]["mall_order_id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/mall-order-facts",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "source_event_id": "evt-cancel-1",
                    "inbox_message_id": "inbox-cancel-1",
                    "business_fact_key": "mall-a:CANCEL:SO-1:v2",
                    "fact_type": "ORDER_CANCELED",
                    "external_order_no": "SO-1",
                    "external_order_version": "v2",
                    "after_sales_request_id": "asr-1",
                    "original_payment_fact_id": payment_fact_id,
                    "occurred_at": 1_750_000_100,
                    "received_at": 1_750_000_110,
                    "data_source": "realtime",
                    "cancel": {
                        "cancel_version": "v2",
                        "cancel_scope": "whole_order",
                        "actual_canceled_quantity": "1.000000",
                        "actual_canceled_amount": "50.00",
                        "reason": "员工取消"
                    }
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["fact_type"], "ORDER_CANCELED");

        let cancel_facts = test_db
            .db()
            .collection::<Document>("mall_order_cancel_facts")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(cancel_facts, 1, "取消扩展事实落库");

        // 完成事实。
        let (status, body) = api
            .post(
                "/admin/mall-order-facts",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "source_event_id": "evt-complete-1",
                    "inbox_message_id": "inbox-complete-1",
                    "business_fact_key": "mall-a:COMPLETE:SO-1:v3",
                    "fact_type": "ORDER_COMPLETED",
                    "external_order_no": "SO-1",
                    "external_order_version": "v3",
                    "original_payment_fact_id": payment_fact_id,
                    "occurred_at": 1_750_000_200,
                    "received_at": 1_750_000_210,
                    "data_source": "realtime",
                    "completion": {
                        "completion_version": "v3",
                        "completed_at": 1_750_000_200
                    }
                })),
            )
            .await;
        assert_ok_envelope(status, &body);

        // 详情含三类事实，取消事实携带售后请求与原支付。
        let (status, body) = api
            .get(&format!("/admin/mall-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let facts = body["data"]["facts"].as_array().unwrap();
        assert_eq!(facts.len(), 3);
        let cancel = facts
            .iter()
            .find(|fact| fact["fact_type"] == "ORDER_CANCELED")
            .expect("取消事实必须存在");
        assert_eq!(cancel["after_sales_request_id"], "asr-1");
        assert_eq!(cancel["original_payment_fact_id"], payment_fact_id);

        // 未关联原支付的取消被拒（422）。
        let (status, body) = api
            .post(
                "/admin/mall-order-facts",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "source_event_id": "evt-cancel-bad",
                    "inbox_message_id": "inbox-cancel-bad",
                    "business_fact_key": "mall-a:CANCEL:SO-9:v1",
                    "fact_type": "ORDER_CANCELED",
                    "external_order_no": "SO-9",
                    "external_order_version": "v1",
                    "after_sales_request_id": "asr-9",
                    "original_payment_fact_id": "fact-missing",
                    "occurred_at": 1_750_000_300,
                    "received_at": 1_750_000_310,
                    "data_source": "realtime",
                    "cancel": {
                        "cancel_version": "v1",
                        "cancel_scope": "whole_order",
                        "actual_canceled_quantity": "1.000000",
                        "actual_canceled_amount": "1.00",
                        "reason": "x"
                    }
                })),
            )
            .await;
        assert_eq!(status, 422, "原支付缺失必须 422: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_and_forbidden_requests() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_order_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, _) = api.get("/admin/mall-orders", None).await;
        assert_eq!(status, 401);

        let test_db = TestDb::new("mall_order_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, body) = api.get("/admin/mall-orders", Some(&token)).await;
        assert_eq!(status, 403, "无权限必须 403: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn invalid_payloads_return_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_order_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // 空白必填字段 → 400。
        let mut payload = payment_payload("v");
        payload["business_fact_key"] = json!("   ");
        let (status, body) = api
            .post("/admin/mall-order-facts", Some(&token), Some(payload))
            .await;
        assert_eq!(status, 400, "空白业务事实键必须 400: {body}");
        assert_eq!(body["data"], Value::Null);

        // 缺字段 → 422（axum Json 拒绝）。
        let (status, _) = api
            .post(
                "/admin/mall-order-facts",
                Some(&token),
                Some(json!({ "mall_id": "mall-a" })),
            )
            .await;
        assert_eq!(status, 422);

        // 非法枚举 → 422。
        let mut payload = payment_payload("v2");
        payload["fact_type"] = json!("PAYING");
        let (status, _) = api
            .post("/admin/mall-order-facts", Some(&token), Some(payload))
            .await;
        assert_eq!(status, 422);

        // 退款事实不属于 D29 接收面 → 422。
        let mut payload = payment_payload("v3");
        payload["fact_type"] = json!("REFUND_SUCCEEDED");
        let (status, body) = api
            .post("/admin/mall-order-facts", Some(&token), Some(payload))
            .await;
        assert_eq!(status, 422, "退款事实必须走售后域接口: {body}");

        // 非法排序字段 → 400。
        let (status, body) = api.get("/admin/mall-orders?sort_by=unknown", Some(&token)).await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn mall_order_list_pagination_bounds() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_order_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_card_instance(test_db.db()).await;

        for suffix in ["a", "b"] {
            let (status, body) = api
                .post(
                    "/admin/mall-order-facts",
                    Some(&token),
                    Some(payment_payload(suffix)),
                )
                .await;
            assert_ok_envelope(status, &body);
        }

        let (status, body) = api
            .get(
                "/admin/mall-orders?mall_id=mall-a&page=1&page_size=1&sort_by=paid_at&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["page_size"], 1);

        let (status, body) = api
            .get("/admin/mall-orders?page=2&page_size=1", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);

        let (status, body) = api
            .get("/admin/mall-orders?page=999&page_size=10", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0, "越界页返回空");

        let (status, body) = api.get("/admin/mall-order-facts?page_size=0", Some(&token)).await;
        assert_eq!(status, 400, "分页大小必须 ≥1: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn legacy_manual_chain_without_cutover() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_order_api_legacy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // 未启用切换：支付归为 LEGACY_MANUAL，只记账不自动下单。
        let (status, body) = api
            .post(
                "/admin/mall-order-facts",
                Some(&token),
                Some(payment_payload("1")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let order_id = body["data"]["mall_order_id"].as_str().unwrap().to_string();
        let (status, body) = api
            .get(&format!("/admin/mall-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["fulfillment"]["chain"], "LEGACY_MANUAL");
        assert_eq!(
            body["data"]["customer"]["attribution_status"],
            "pending_attribution"
        );
    })
}
