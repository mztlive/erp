//! 域 D25 `supplier_api` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test supplier_api_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入本域 `p` 规则
//! （casbin `g(sub, sub)` 自反匹配），使 happy path 可鉴权通过，同时天然构造 403。
//!
//! 外部 HTTP 调用（健康检查）不真调外部网络：handler 使用默认失败关闭网关，
//! 覆盖「失败降级为可观测错误」路径（`inbox_message` + `integration_error_task`）；
//! mock 成功路径在 services 单测覆盖。

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
use axum::Router;
use config::{Config, SafeConfig};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use tower::ServiceExt;
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节，同时满足 config 校验与 test-support 签发要求）。
const TEST_JWT_SECRET: &str = "p0-5-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("supplier_api_connection", "list"),
    ("supplier_api_connection", "detail"),
    ("supplier_api_connection", "create"),
    ("supplier_api_connection", "update"),
    ("supplier_api_connection", "health_check"),
    ("supplier_api_capability", "list"),
];

/// 发送 PUT 请求（`TestApi` 只提供 GET/POST，PUT 路径由本辅助覆盖）。
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

/// 种子一个启用状态的供应商账号（D09 `supplier_account`，本域创建校验依赖）。
async fn seed_supplier_account(db: &Database) -> String {
    let supplier = entities::supplier::SupplierAccount::new(
        entities::ids::SupplierAccountId::new("sup-test-1"),
        entities::supplier::SupplierAccountData {
            party_id: entities::ids::PartyId::new("party-1"),
            supplier_no: "SUP-0001".to_string(),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: None,
            status: entities::supplier::SupplierAccountStatus::Active,
        },
        "admin-1",
    )
    .expect("供应商账号构造失败");
    let id = supplier.base.id.clone();
    db.collection::<entities::supplier::SupplierAccount>("supplier_accounts")
        .insert_one(&supplier)
        .await
        .expect("供应商账号种子失败");
    id
}

/// 构造最小 AppState（默认配置 + 临时上传目录）并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "supplier-api-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("supplier-api-uploads-{}", uuid::Uuid::new_v4()));
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

/// 组装「建库 + 索引 + 种子账号 + 权限 + token + 路由」的公共前置。
async fn setup(prefix: &str) -> (TestDb, String, Router) {
    let test_db = TestDb::new(prefix).await.unwrap();
    database::ensure_indexes(test_db.db()).await.unwrap();
    let account_id = seed_admin_account(test_db.db()).await.unwrap();
    grant_domain_permissions(test_db.db(), &account_id).await;
    let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
    let (router, _) = build_router(&test_db).await;
    (test_db, token, router)
}

/// 创建连接并返回响应视图（供后续用例复用）。
async fn create_connection(api: &TestApi, token: &str, code: &str) -> (u16, Value) {
    let body = json!({
        "supplier_id": "sup-test-1",
        "connection_code": code,
        "environment": "production",
        "endpoint_reference": "config://supplier/001",
        "credential_reference": "kms://prod/erp/sup-001",
        "rate_limit_policy": { "max_requests": 100, "window_secs": 60 },
        "status": "active",
        "capabilities": [
            { "capability_code": "product", "status": "active", "constraint_snapshot": "单笔上限 50000 元" },
            { "capability_code": "order", "status": "active" }
        ]
    });
    api.post("/admin/supplier-api-connections", Some(token), Some(body))
        .await
}

#[tokio::test]
#[ignore]
async fn happy_path_create_connection_then_list_and_detail_with_contract_shape() {
    require_mongo!(async {
        let (test_db, token, router) = setup("sup_api_happy").await;
        seed_supplier_account(test_db.db()).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/supplier-api-connections", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["items"], json!([]), "初始列表为空");
        assert_eq!(data["total"], 0);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);

        let (status, body) = create_connection(&api, &token, "CONN-001").await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        for field in [
            "id",
            "supplier_id",
            "connection_code",
            "environment",
            "status",
            "created_at",
        ] {
            assert!(
                created.get(field).is_some(),
                "契约字段 {field} 必须存在: {created}"
            );
        }
        assert_eq!(created["connection_code"], "CONN-001");
        assert_eq!(created["environment"], "production");
        assert_eq!(created["status"], "active");
        assert_eq!(created["version"], 1);
        assert!(created.get("credential_reference").is_none(), "密钥引用永不回显");

        let id = created["id"].as_str().unwrap();
        let (status, body) = api.get("/admin/supplier-api-connections", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["items"][0]["connection_code"], "CONN-001");

        let (status, body) = api
            .get(&format!("/admin/supplier-api-connections/{id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["connection"]["connection_code"], "CONN-001");
        assert_eq!(detail["capabilities"].as_array().unwrap().len(), 2);
        assert_eq!(detail["capabilities"][0]["capability_code"], "product");
        assert_eq!(detail["capabilities"][0]["status"], "active");

        let (status, body) = api
            .get(
                &format!("/admin/supplier-api-capabilities?connection_id={id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2, "按连接筛选能力应命中 2 条");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/supplier-api-connections", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("sup_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/supplier-api-connections", Some(&token)).await;
        assert_eq!(
            status, 403,
            "无 supplier_api_connection.list 权限必须 403: {body}"
        );
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let (test_db, token, router) = setup("sup_api_400").await;
        seed_supplier_account(test_db.db()).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/supplier-api-connections",
                Some(&token),
                Some(json!({
                    "supplier_id": "sup-test-1",
                    "connection_code": "   ",
                    "environment": "production",
                    "endpoint_reference": "config://supplier/001"
                })),
            )
            .await;
        assert_eq!(status, 400, "空白连接代码必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null, "失败时 data 恒为 null");

        let (status, _) = api
            .post(
                "/admin/supplier-api-connections",
                Some(&token),
                Some(json!({ "connection_code": "CN-1" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/supplier-api-connections",
                Some(&token),
                Some(json!({
                    "supplier_id": "sup-test-1",
                    "connection_code": "CN-1",
                    "environment": "MARS",
                    "endpoint_reference": "config://supplier/001"
                })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_and_duplicate_code_return_409() {
    require_mongo!(async {
        let (test_db, token, router) = setup("sup_api_409").await;
        seed_supplier_account(test_db.db()).await;
        let api = TestApi::new(router.clone());

        let (_, body) = create_connection(&api, &token, "CONN-409").await;
        assert_ok_envelope(200, &body);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = create_connection(&api, &token, "CONN-409").await;
        assert_eq!(status, 409, "重复 connection_code 唯一索引冲突必须 409: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = put_json(
            &router,
            &format!("/admin/supplier-api-connections/{id}"),
            &token,
            json!({ "version": 1, "status": "disabled" }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "disabled");
        assert_eq!(body["data"]["version"], 2, "更新成功版本递增");

        let (status, body) = put_json(
            &router,
            &format!("/admin/supplier-api-connections/{id}"),
            &token,
            json!({ "version": 1, "status": "active" }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本更新必须 409: {body}");
        assert_eq!(body["data"], Value::Null);
    })
}

#[tokio::test]
#[ignore]
async fn replace_capabilities_is_atomic_and_rejects_stale_version() {
    require_mongo!(async {
        let (test_db, token, router) = setup("sup_api_caps").await;
        seed_supplier_account(test_db.db()).await;
        let api = TestApi::new(router.clone());

        let (_, body) = create_connection(&api, &token, "CONN-CAPS").await;
        assert_ok_envelope(200, &body);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = put_json(
            &router,
            &format!("/admin/supplier-api-connections/{id}/capabilities"),
            &token,
            json!({
                "expected_connection_version": 1,
                "capabilities": [
                    { "capability_code": "stock", "status": "active" },
                    { "capability_code": "refund", "status": "disabled" }
                ]
            }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"].as_array().unwrap().len(), 2, "替换后能力清单为 2 条");
        let codes: Vec<&str> = body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["capability_code"].as_str().unwrap())
            .collect();
        assert!(codes.contains(&"stock"));
        assert!(!codes.contains(&"product"), "旧能力 product 已被替换");

        let (status, body) = put_json(
            &router,
            &format!("/admin/supplier-api-connections/{id}/capabilities"),
            &token,
            json!({
                "expected_connection_version": 1,
                "capabilities": [{ "capability_code": "stock", "status": "disabled" }]
            }),
        )
        .await;
        assert_eq!(status, 409, "陈旧连接版本替换必须 409: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn health_check_failure_degrades_to_observable_error_and_is_idempotent() {
    require_mongo!(async {
        let (test_db, token, router) = setup("sup_api_health").await;
        seed_supplier_account(test_db.db()).await;
        let api = TestApi::new(router);

        let (_, body) = create_connection(&api, &token, "CONN-HEALTH").await;
        assert_ok_envelope(200, &body);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/supplier-api-connections/{id}/health-check"),
                Some(&token),
                Some(json!({ "idempotency_key": "hk-001" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let result = &body["data"];
        assert_eq!(result["result"], "failed", "默认网关失败关闭 → 健康检查失败");
        assert!(result["error_task_id"].as_str().unwrap().len() > 0);
        assert!(!result["inbox_message_id"].as_str().unwrap().is_empty());

        let inbox_count = test_db
            .db()
            .collection::<Document>("inbox_messages")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(inbox_count, 1, "一次健康检查只落一条消息信封");
        let inbox = test_db
            .db()
            .collection::<Document>("inbox_messages")
            .find_one(doc! {})
            .await
            .unwrap()
            .expect("消息信封必须存在");
        assert_eq!(inbox.get_str("status").unwrap(), "failed");

        let task_count = test_db
            .db()
            .collection::<Document>("integration_error_tasks")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(task_count, 1, "失败必须创建一条集成错误任务");
        let task = test_db
            .db()
            .collection::<Document>("integration_error_tasks")
            .find_one(doc! {})
            .await
            .unwrap()
            .expect("错误任务必须存在");
        assert_eq!(task.get_str("error_class").unwrap(), "transient_failure");

        let (status, body) = api
            .post(
                &format!("/admin/supplier-api-connections/{id}/health-check"),
                Some(&token),
                Some(json!({ "idempotency_key": "hk-001" })),
            )
            .await;
        assert_eq!(status, 409, "同幂等键重复健康检查必须 409: {body}");
        let inbox_count = test_db
            .db()
            .collection::<Document>("inbox_messages")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(inbox_count, 1, "重复提交不产生第二条消息信封");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_boundaries_are_enforced() {
    require_mongo!(async {
        let (test_db, token, router) = setup("sup_api_page").await;
        seed_supplier_account(test_db.db()).await;
        let api = TestApi::new(router);

        let (status, _) = api
            .get("/admin/supplier-api-connections?page=0", Some(&token))
            .await;
        assert_eq!(status, 400, "页码 0 必须 400");

        let (status, _) = api
            .get("/admin/supplier-api-connections?page_size=101", Some(&token))
            .await;
        assert_eq!(status, 400, "分页大小超界必须 400");

        let (status, body) = api
            .get(
                "/admin/supplier-api-connections?sort_by=secret_field",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");
    })
}
