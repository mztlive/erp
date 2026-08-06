//! 域 D28 `card_instance` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）。
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外插入本域直接 `p` 规则
//! （casbin `g(r.sub, p.sub)` 自反匹配），使 happy path 可鉴权通过，
//! 同时天然构造 403 用例。

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
use axum::Router;
use config::{Config, SafeConfig};
use entities::ids::{CustomerAccountId, PartyId, SalesOrderId};
use entities::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use tower::ServiceExt;
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节）。
const TEST_JWT_SECRET: &str = "c-g10-card-instance-test-secret-32-bytes-min";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("mall_consumption_cutover", "list"),
    ("mall_consumption_cutover", "create"),
    ("mall_consumption_cutover", "detail"),
    ("mall_consumption_cutover", "submit"),
    ("mall_card_instance", "list"),
    ("mall_card_instance", "create"),
    ("mall_card_instance", "detail"),
    ("mall_balance_snapshot", "list"),
    ("mall_balance_snapshot", "create"),
    ("mall_card_instance_correction", "list"),
];

/// 发送 PUT 请求（`TestApi` 只提供 GET/POST）。
async fn put_json(router: &Router, path: &str, token: &str, json: Value) -> (u16, Value) {
    let request = Request::builder()
        .method(Method::PUT)
        .uri(path)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        )
        .header("content-type", "application/json")
        .body(Body::from(json.to_string()))
        .expect("PUT 请求构造失败");
    let response = router.clone().oneshot(request).await.expect("路由调用失败");
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("响应体读取失败");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

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

/// 种子原销售单（D13 实体，`create_card_instance` 经 D13 仓储校验存在）。
async fn seed_sales_order(db: &Database) -> String {
    let order = SalesOrder::new(
        SalesOrderId::new("so-card-test-1"),
        SalesOrderData {
            order_no: "SO-CARD-2025-0001".to_string(),
            business_type: BusinessType::Voucher,
            origin_system: OriginSystem::Mall,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: None,
            settlement_party_id: PartyId::new("party-1"),
            source_status_code: None,
        },
        "tester",
    )
    .expect("销售单实体构造失败");
    let id = order.base.id.clone();
    db.collection::<SalesOrder>("sales_orders")
        .insert_one(&order)
        .await
        .expect("销售单种子写入失败");
    id
}

#[tokio::test]
#[ignore]
async fn happy_path_cutover_enable_then_card_instance_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("card_inst_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        // 切换记录：创建 → 列表 → 启用 → 详情。
        let (status, body) = api
            .post(
                "/admin/consumption-cutovers",
                Some(&token),
                Some(json!({ "mall_id": "mall-a", "checklist_reference": "doc-1" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let cutover = &body["data"];
        assert_eq!(cutover["mall_id"], "mall-a");
        assert_eq!(cutover["status"], "preparing");
        assert_eq!(cutover["version"], 1);
        let cutover_id = cutover["id"].as_str().unwrap().to_string();

        let (status, body) = api.get("/admin/consumption-cutovers", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["page"], 1);
        assert_eq!(body["data"]["page_size"], 20);
        for field in ["id", "mall_id", "status", "created_at", "version"] {
            assert!(
                body["data"]["items"][0].get(field).is_some(),
                "契约字段 {field} 必须存在"
            );
        }

        let (status, body) = put_json(
            &router,
            &format!("/admin/consumption-cutovers/{cutover_id}/enable"),
            &token,
            json!({ "version": 1, "enabled_at": 1_750_000_000 }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "enabled");
        assert_eq!(body["data"]["enabled_at"], 1_750_000_000);
        assert_eq!(body["data"]["enabled_by"], account_id);

        let (status, body) = api
            .get(&format!("/admin/consumption-cutovers/{cutover_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "enabled");

        // 卡实例：原销售单种子 + 基线创建 → 列表 → 详情 → 快照。
        let sales_order_id = seed_sales_order(test_db.db()).await;
        let (status, body) = api
            .post(
                "/admin/card-instances",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "opaque_instance_ref": "card-ref-001",
                    "origin_sales_order_source_identity_id": "ext-map-1",
                    "origin_sales_order_id": sales_order_id,
                    "origin_sales_order_revision_id": "so-rev-1",
                    "source_baseline_version": "v3",
                    "initial_balance": "500.00",
                    "baseline_at": 1_749_000_000,
                    "source_type": "realtime",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let instance = &body["data"];
        assert_eq!(instance["mall_id"], "mall-a");
        assert_eq!(instance["opaque_instance_ref"], "card-ref-001");
        assert_eq!(instance["initial_balance"], "500.00");
        assert_eq!(instance["source_type"], "realtime");
        let instance_id = instance["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .get("/admin/card-instances?mall_id=mall-a", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["initial_balance"], "500.00");

        let (status, body) = api
            .get(&format!("/admin/card-instances/{instance_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["latest_balance"], "500.00");
        assert_eq!(body["data"]["balance_snapshot_count"], 1);
        assert_eq!(body["data"]["correction_count"], 0);

        let (status, body) = api
            .post(
                &format!("/admin/card-instances/{instance_id}/balance-snapshots"),
                Some(&token),
                Some(json!({
                    "mall_card_instance_id": instance_id,
                    "snapshot_at": 1_750_000_100,
                    "balance": "420.00",
                    "source_snapshot_version": "v4",
                    "source_event_id": "evt-snapshot-2",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["balance"], "420.00");

        let (status, body) = api
            .get(
                &format!(
                    "/admin/card-instances/{instance_id}/balance-snapshots?sort_by=snapshot_at&sort_dir=desc"
                ),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2);
        assert_eq!(body["data"]["items"][0]["balance"], "420.00");

        let (status, body) = api
            .get(
                &format!("/admin/card-instances/{instance_id}/corrections"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0);

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("card_inst_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/card-instances", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("card_inst_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/consumption-cutovers", Some(&token)).await;
        assert_eq!(status, 403, "无本域权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400() {
    require_mongo!(async {
        let test_db = TestDb::new("card_inst_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/consumption-cutovers",
                Some(&token),
                Some(json!({ "mall_id": "   ", "checklist_reference": null })),
            )
            .await;
        assert_eq!(status, 400, "空白 mall_id 必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, _) = api
            .post(
                "/admin/consumption-cutovers",
                Some(&token),
                Some(json!({ "checklist_reference": null })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, body) = api.get("/admin/card-instances?sort_by=evil", Some(&token)).await;
        assert_eq!(status, 400, "排序字段不在白名单必须 400: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .get("/admin/consumption-cutovers?page_size=999", Some(&token))
            .await;
        assert_eq!(status, 400, "分页大小越界必须 400: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_enable_returns_409_and_single_t_per_mall() {
    require_mongo!(async {
        let test_db = TestDb::new("card_inst_api_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (_, body) = api
            .post(
                "/admin/consumption-cutovers",
                Some(&token),
                Some(json!({ "mall_id": "mall-a" })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = put_json(
            &router,
            &format!("/admin/consumption-cutovers/{id}/enable"),
            &token,
            json!({ "version": 1, "enabled_at": 1_750_000_000 }),
        )
        .await;
        assert_ok_envelope(status, &body);

        let (status, body) = put_json(
            &router,
            &format!("/admin/consumption-cutovers/{id}/enable"),
            &token,
            json!({ "version": 1, "enabled_at": 1_750_000_100 }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本启用必须 409: {body}");
        assert_eq!(body["success"], false);

        // 同一商城第二份切换记录启用被拒。
        let (_, body) = api
            .post(
                "/admin/consumption-cutovers",
                Some(&token),
                Some(json!({ "mall_id": "mall-a" })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let second_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, body) = put_json(
            &router,
            &format!("/admin/consumption-cutovers/{second_id}/enable"),
            &token,
            json!({ "version": 1, "enabled_at": 1_750_000_200 }),
        )
        .await;
        assert_eq!(status, 409, "同一商城只能有一个启用 T: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_card_instance_same_identity_conflicts() {
    require_mongo!(async {
        let test_db = TestDb::new("card_inst_api_dup").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let sales_order_id = seed_sales_order(test_db.db()).await;

        let payload = json!({
            "mall_id": "mall-a",
            "opaque_instance_ref": "card-ref-dup",
            "origin_sales_order_source_identity_id": "ext-map-1",
            "origin_sales_order_id": sales_order_id,
            "origin_sales_order_revision_id": "so-rev-1",
            "initial_balance": "500.00",
            "baseline_at": 1_749_000_000,
            "source_type": "realtime",
        });
        let (status, body) = api
            .post("/admin/card-instances", Some(&token), Some(payload.clone()))
            .await;
        assert_ok_envelope(status, &body);

        // 完全一致重复基线 → 幂等确认，不新增。
        let (status, body) = api
            .post("/admin/card-instances", Some(&token), Some(payload.clone()))
            .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api.get("/admin/card-instances", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1, "重复基线只确认接收，不新增卡实例");

        // 同身份不同余额 → 冲突。
        let conflict = json!({
            "mall_id": "mall-a",
            "opaque_instance_ref": "card-ref-dup",
            "origin_sales_order_source_identity_id": "ext-map-1",
            "origin_sales_order_id": sales_order_id,
            "origin_sales_order_revision_id": "so-rev-1",
            "initial_balance": "600.00",
            "baseline_at": 1_749_000_000,
            "source_type": "realtime",
        });
        let (status, body) = api
            .post("/admin/card-instances", Some(&token), Some(conflict))
            .await;
        assert_eq!(status, 409, "同身份冲突基线必须 409: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn cutover_pagination_and_sort_whitelist() {
    require_mongo!(async {
        let test_db = TestDb::new("card_inst_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        for mall in ["mall-a", "mall-b", "mall-c"] {
            let (status, body) = api
                .post(
                    "/admin/consumption-cutovers",
                    Some(&token),
                    Some(json!({ "mall_id": mall })),
                )
                .await;
            assert_ok_envelope(status, &body);
        }

        let (status, body) = api
            .get(
                "/admin/consumption-cutovers?page=1&page_size=2&sort_by=created_at&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 2);
        assert_eq!(body["data"]["page_size"], 2);

        let (status, body) = api
            .get("/admin/consumption-cutovers?page=2&page_size=2", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);

        let (status, body) = api
            .get("/admin/consumption-cutovers?sort_by=unknown_field", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
    })
}
