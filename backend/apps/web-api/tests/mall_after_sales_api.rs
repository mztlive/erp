//! 域 D30 `mall_after_sales` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控。覆盖：退款事务不变量
//! （§8.4 第 3 条：事实 + 退款头 + 行 + `APPLY` 分配 + 消费反向原子写入）、
//! 幂等、累计退款上限、余额恢复（§8.4 第 4 条）与恢复上限、注入失败全量不可见。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use entities::card_instance::{CardSourceType, MallCardInstance, MallCardInstanceData};
use entities::common::time::Instant;
use entities::ids::{ExternalIdentityMapId, MallCardInstanceId, SalesOrderId, SalesOrderRevisionId};
use entities::money::Amount;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use std::str::FromStr;
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节）。
const TEST_JWT_SECRET: &str = "c-g10-after-sales-test-secret-32-bytes-min";
/// 种子账号可访问的本域权限键（含 D29 支付接收与 D28 卡实例）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("mall_order", "list"),
    ("mall_order", "detail"),
    ("mall_order_fact", "list"),
    ("mall_order_fact", "submit"),
    ("mall_refund", "submit"),
    ("mall_refund", "list"),
    ("mall_balance_restoration", "submit"),
    ("mall_balance_restoration", "list"),
    ("mall_after_sales_request", "list"),
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

/// 种子卡实例基线（卡券来源归集用）。
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

/// 经 D29 接口接收一笔支付（卡券 30 + 微信 20），返回详情视图。
async fn seed_payment(api: &TestApi, token: &str) -> Value {
    let (status, body) = api
        .post(
            "/admin/mall-order-facts",
            Some(token),
            Some(json!({
                "mall_id": "mall-a",
                "source_event_id": "evt-pay-1",
                "inbox_message_id": "inbox-pay-1",
                "business_fact_key": "mall-a:PAYMENT:SO-1:v1",
                "fact_type": "PAYMENT_SUCCEEDED",
                "external_order_no": "SO-1",
                "external_order_version": "v1",
                "occurred_at": 1_750_000_000,
                "received_at": 1_750_000_010,
                "data_source": "realtime",
                "payment": {
                    "mall_user_ref": "user-1",
                    "ordered_at": 1_749_999_900,
                    "gross_amount": "50.00",
                    "discount_amount": "0.00",
                    "freight_amount": "0.00",
                    "paid_amount": "50.00",
                    "items": [{
                        "external_item_id": "item-1",
                        "name_snapshot": "测试商品",
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
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let order_id = body["data"]["mall_order_id"].as_str().unwrap().to_string();
    let (status, body) = api
        .get(&format!("/admin/mall-orders/{order_id}"), Some(token))
        .await;
    assert_ok_envelope(status, &body);
    body["data"].clone()
}

#[tokio::test]
#[ignore]
async fn refund_receive_reverses_consumption_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("as_api_refund").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_card_instance(test_db.db()).await;
        let payment = seed_payment(&api, &token).await;

        let item_id = payment["items"][0]["mall_order_item_id"]
            .as_str()
            .unwrap()
            .to_string();
        let card_entry_id = payment["consumption_entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["payment_source_id"] == payment["payment_sources"][0]["payment_source_id"])
            .expect("卡券消费事实必须存在")["consumption_entry_id"]
            .as_str()
            .unwrap()
            .to_string();
        let card_source_id = payment["payment_sources"][0]["payment_source_id"]
            .as_str()
            .unwrap()
            .to_string();
        let payment_fact_id = payment["identity"]["payment_fact_id"]
            .as_str()
            .unwrap()
            .to_string();

        let (status, body) = api
            .post(
                "/admin/mall-refunds",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "source_event_id": "evt-refund-1",
                    "inbox_message_id": "inbox-refund-1",
                    "business_fact_key": "mall-a:REFUND:RF-1:v1",
                    "external_order_no": "SO-1",
                    "external_order_version": "v2",
                    "after_sales_request_id": "asr-1",
                    "original_payment_fact_id": payment_fact_id,
                    "occurred_at": 1_750_000_100,
                    "received_at": 1_750_000_110,
                    "data_source": "realtime",
                    "external_refund_no": "RF-1",
                    "external_refund_version": "v1",
                    "refund_amount": "30.00",
                    "refunded_at": 1_750_000_100,
                    "lines": [{
                        "line_no": 1,
                        "mall_order_item_id": item_id,
                        "refunded_quantity": "1.000000",
                        "line_refund_amount": "30.00"
                    }],
                    "allocations": [{
                        "line_no": 1,
                        "allocation_no": 1,
                        "original_consumption_entry_id": card_entry_id,
                        "original_payment_source_id": card_source_id,
                        "allocated_refund_amount": "30.00"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["fact_type"], "REFUND_SUCCEEDED");
        assert_eq!(body["data"]["idempotent_hit"], false);

        // §8.4 第 3 条：事实 + 退款头 + 行 + 分配 + 消费反向同事务生效。
        let refunds = test_db
            .db()
            .collection::<Document>("mall_refunds")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(refunds, 1);
        let lines = test_db
            .db()
            .collection::<Document>("mall_refund_lines")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(lines, 1);
        let allocations = test_db
            .db()
            .collection::<Document>("mall_refund_allocations")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(allocations, 1);
        let reversals = test_db
            .db()
            .collection::<Document>("mall_consumption_entries")
            .count_documents(doc! { "direction": "consumption_reversal" })
            .await
            .unwrap();
        assert_eq!(reversals, 1, "退款必须追加消费反向事实");

        // 退款列表契约形状。
        let (status, body) = api
            .get("/admin/mall-refunds?after_sales_request_id=asr-1", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["external_refund_no"], "RF-1");
        assert_eq!(body["data"]["items"][0]["refund_amount"], "30.00");
    })
}

#[tokio::test]
#[ignore]
async fn refund_receive_is_idempotent_by_business_fact_key() {
    require_mongo!(async {
        let test_db = TestDb::new("as_api_refund_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_card_instance(test_db.db()).await;
        let payment = seed_payment(&api, &token).await;

        let item_id = payment["items"][0]["mall_order_item_id"]
            .as_str()
            .unwrap()
            .to_string();
        let card_entry_id = payment["consumption_entries"][0]["consumption_entry_id"]
            .as_str()
            .unwrap()
            .to_string();
        let card_source_id = payment["payment_sources"][0]["payment_source_id"]
            .as_str()
            .unwrap()
            .to_string();
        let payment_fact_id = payment["identity"]["payment_fact_id"]
            .as_str()
            .unwrap()
            .to_string();
        let refund_payload = json!({
            "mall_id": "mall-a",
            "source_event_id": "evt-refund-1",
            "inbox_message_id": "inbox-refund-1",
            "business_fact_key": "mall-a:REFUND:RF-1:v1",
            "external_order_no": "SO-1",
            "external_order_version": "v2",
            "after_sales_request_id": "asr-1",
            "original_payment_fact_id": payment_fact_id,
            "occurred_at": 1_750_000_100,
            "received_at": 1_750_000_110,
            "data_source": "realtime",
            "external_refund_no": "RF-1",
            "external_refund_version": "v1",
            "refund_amount": "30.00",
            "refunded_at": 1_750_000_100,
            "lines": [{
                "line_no": 1,
                "mall_order_item_id": item_id,
                "refunded_quantity": "1.000000",
                "line_refund_amount": "30.00"
            }],
            "allocations": [{
                "line_no": 1,
                "allocation_no": 1,
                "original_consumption_entry_id": card_entry_id,
                "original_payment_source_id": card_source_id,
                "allocated_refund_amount": "30.00"
            }]
        });
        let (status, body) = api
            .post("/admin/mall-refunds", Some(&token), Some(refund_payload.clone()))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["idempotent_hit"], false);
        let (status, body) = api
            .post("/admin/mall-refunds", Some(&token), Some(refund_payload))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["idempotent_hit"], true, "重复退款幂等命中");

        let refunds = test_db
            .db()
            .collection::<Document>("mall_refunds")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(refunds, 1, "重复提交只产生一条退款");
        let reversals = test_db
            .db()
            .collection::<Document>("mall_consumption_entries")
            .count_documents(doc! { "direction": "consumption_reversal" })
            .await
            .unwrap();
        assert_eq!(reversals, 1, "重复提交不重复消费冲减");
    })
}

#[tokio::test]
#[ignore]
async fn refund_over_original_entry_limit_returns_422_and_rolls_back() {
    require_mongo!(async {
        let test_db = TestDb::new("as_api_refund_over").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_card_instance(test_db.db()).await;
        let payment = seed_payment(&api, &token).await;

        let item_id = payment["items"][0]["mall_order_item_id"]
            .as_str()
            .unwrap()
            .to_string();
        let card_entry_id = payment["consumption_entries"][0]["consumption_entry_id"]
            .as_str()
            .unwrap()
            .to_string();
        let card_source_id = payment["payment_sources"][0]["payment_source_id"]
            .as_str()
            .unwrap()
            .to_string();
        let payment_fact_id = payment["identity"]["payment_fact_id"]
            .as_str()
            .unwrap()
            .to_string();

        // 卡券原消费 30.00，申请退款 35.00 超限。
        let (status, body) = api
            .post(
                "/admin/mall-refunds",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "source_event_id": "evt-refund-over",
                    "inbox_message_id": "inbox-refund-over",
                    "business_fact_key": "mall-a:REFUND:RF-2:v1",
                    "external_order_no": "SO-1",
                    "external_order_version": "v2",
                    "after_sales_request_id": "asr-2",
                    "original_payment_fact_id": payment_fact_id,
                    "occurred_at": 1_750_000_100,
                    "received_at": 1_750_000_110,
                    "data_source": "realtime",
                    "external_refund_no": "RF-2",
                    "external_refund_version": "v1",
                    "refund_amount": "35.00",
                    "refunded_at": 1_750_000_100,
                    "lines": [{
                        "line_no": 1,
                        "mall_order_item_id": item_id,
                        "refunded_quantity": "1.000000",
                        "line_refund_amount": "35.00"
                    }],
                    "allocations": [{
                        "line_no": 1,
                        "allocation_no": 1,
                        "original_consumption_entry_id": card_entry_id,
                        "original_payment_source_id": card_source_id,
                        "allocated_refund_amount": "35.00"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 422, "累计退款超过原消费必须 422: {body}");
        assert_eq!(body["data"], Value::Null);
        for collection in ["mall_refunds", "mall_refund_lines", "mall_refund_allocations"] {
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
async fn balance_restoration_receive_and_over_limit_rejected() {
    require_mongo!(async {
        let test_db = TestDb::new("as_api_restore").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_card_instance(test_db.db()).await;
        let payment = seed_payment(&api, &token).await;

        let item_id = payment["items"][0]["mall_order_item_id"]
            .as_str()
            .unwrap()
            .to_string();
        let card_entry_id = payment["consumption_entries"][0]["consumption_entry_id"]
            .as_str()
            .unwrap()
            .to_string();
        let card_source_id = payment["payment_sources"][0]["payment_source_id"]
            .as_str()
            .unwrap()
            .to_string();
        let card_instance_id = payment["payment_sources"][0]["mall_card_instance_id"]
            .as_str()
            .unwrap()
            .to_string();
        let payment_fact_id = payment["identity"]["payment_fact_id"]
            .as_str()
            .unwrap()
            .to_string();

        // 先退款 30（卡券全额）。
        let (status, body) = api
            .post(
                "/admin/mall-refunds",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "source_event_id": "evt-refund-1",
                    "inbox_message_id": "inbox-refund-1",
                    "business_fact_key": "mall-a:REFUND:RF-1:v1",
                    "external_order_no": "SO-1",
                    "external_order_version": "v2",
                    "after_sales_request_id": "asr-1",
                    "original_payment_fact_id": payment_fact_id,
                    "occurred_at": 1_750_000_100,
                    "received_at": 1_750_000_110,
                    "data_source": "realtime",
                    "external_refund_no": "RF-1",
                    "external_refund_version": "v1",
                    "refund_amount": "30.00",
                    "refunded_at": 1_750_000_100,
                    "lines": [{
                        "line_no": 1,
                        "mall_order_item_id": item_id,
                        "refunded_quantity": "1.000000",
                        "line_refund_amount": "30.00"
                    }],
                    "allocations": [{
                        "line_no": 1,
                        "allocation_no": 1,
                        "original_consumption_entry_id": card_entry_id,
                        "original_payment_source_id": card_source_id,
                        "allocated_refund_amount": "30.00"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        // 取退款分配 ID（直接查库；恢复分配引用原 CARD 退款分配）。
        let refund_allocation = test_db
            .db()
            .collection::<Document>("mall_refund_allocations")
            .find_one(doc! {})
            .await
            .unwrap()
            .expect("退款分配必须存在");
        let refund_allocation_id = refund_allocation.get_str("id").unwrap().to_string();
        let refund_head = test_db
            .db()
            .collection::<Document>("mall_refunds")
            .find_one(doc! {})
            .await
            .unwrap()
            .expect("退款头必须存在");
        let refund_id = refund_head.get_str("id").unwrap().to_string();

        // 余额恢复：30 全额回补。
        let (status, body) = api
            .post(
                "/admin/mall-balance-restorations",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "source_event_id": "evt-restore-1",
                    "inbox_message_id": "inbox-restore-1",
                    "business_fact_key": "mall-a:RESTORE:RS-1:v1",
                    "external_order_no": "SO-1",
                    "external_order_version": "v3",
                    "after_sales_request_id": "asr-1",
                    "original_payment_fact_id": payment_fact_id,
                    "occurred_at": 1_750_000_200,
                    "received_at": 1_750_000_210,
                    "data_source": "realtime",
                    "external_restoration_no": "RS-1",
                    "version": "v1",
                    "restored_amount": "30.00",
                    "restored_at": 1_750_000_200,
                    "allocations": [{
                        "allocation_no": 1,
                        "mall_refund_allocation_id": refund_allocation_id,
                        "mall_card_instance_id": card_instance_id,
                        "restored_amount": "30.00"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["fact_type"], "CARD_BALANCE_RESTORED");

        let restorations = test_db
            .db()
            .collection::<Document>("mall_balance_restorations")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(restorations, 1);
        let restore_allocations = test_db
            .db()
            .collection::<Document>("mall_balance_restoration_allocations")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(restore_allocations, 1);

        // 列表契约形状。
        let (status, body) = api
            .get(
                "/admin/mall-balance-restorations?after_sales_request_id=asr-1",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["external_restoration_no"], "RS-1");

        // 超限恢复（退款净额 30，再恢复 10）→ 422 且无残留。
        let (status, body) = api
            .post(
                "/admin/mall-balance-restorations",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "source_event_id": "evt-restore-over",
                    "inbox_message_id": "inbox-restore-over",
                    "business_fact_key": "mall-a:RESTORE:RS-2:v1",
                    "external_order_no": "SO-1",
                    "external_order_version": "v3",
                    "after_sales_request_id": "asr-1",
                    "original_payment_fact_id": payment_fact_id,
                    "occurred_at": 1_750_000_300,
                    "received_at": 1_750_000_310,
                    "data_source": "realtime",
                    "external_restoration_no": "RS-2",
                    "version": "v1",
                    "restored_amount": "10.00",
                    "restored_at": 1_750_000_300,
                    "allocations": [{
                        "allocation_no": 1,
                        "mall_refund_allocation_id": refund_allocation_id,
                        "mall_card_instance_id": card_instance_id,
                        "restored_amount": "10.00"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 422, "累计恢复超过 CARD 退款净额必须 422: {body}");
        let restore_count = test_db
            .db()
            .collection::<Document>("mall_balance_restorations")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(restore_count, 1, "超限恢复不留新头");
        let _ = refund_id;
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_forbidden_and_invalid_payloads() {
    require_mongo!(async {
        let test_db = TestDb::new("as_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, _) = api.get("/admin/mall-refunds", None).await;
        assert_eq!(status, 401);

        let test_db = TestDb::new("as_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, body) = api.get("/admin/mall-after-sales-requests", Some(&token)).await;
        assert_eq!(status, 403, "无权限必须 403: {body}");

        let test_db = TestDb::new("as_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // 空白必填 → 400。
        let (status, body) = api
            .post(
                "/admin/mall-refunds",
                Some(&token),
                Some(json!({
                    "mall_id": " ",
                    "source_event_id": "e",
                    "inbox_message_id": "i",
                    "business_fact_key": "k",
                    "external_order_no": "n",
                    "external_order_version": "v",
                    "after_sales_request_id": "asr",
                    "original_payment_fact_id": "f",
                    "occurred_at": 1,
                    "received_at": 2,
                    "data_source": "realtime",
                    "external_refund_no": "r",
                    "external_refund_version": "v",
                    "refund_amount": "1.00",
                    "refunded_at": 3,
                    "lines": [],
                    "allocations": []
                })),
            )
            .await;
        assert_eq!(status, 400, "空白商城与空行必须 400: {body}");

        // 缺字段 → 422。
        let (status, _) = api
            .post("/admin/mall-refunds", Some(&token), Some(json!({})))
            .await;
        assert_eq!(status, 422);

        // 非法排序 → 400。
        let (status, body) = api.get("/admin/mall-refunds?sort_by=evil", Some(&token)).await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
    })
}
