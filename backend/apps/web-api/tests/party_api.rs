//! 域 D07 `party` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test party_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入
//! `party`/`party_contact`/`party_bank_account` 等的直接 `p` 规则，
//! 使 happy path 可鉴权通过，同时天然构造 403 用例。

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
    ("party", "list"),
    ("party", "create"),
    ("party", "detail"),
    ("party", "update"),
    ("party", "delete"),
    ("party_revision", "list"),
    ("party_contact", "list"),
    ("party_contact", "create"),
    ("party_contact", "update"),
    ("party_address", "list"),
    ("party_address", "create"),
    ("party_address", "update"),
    ("party_tax_profile", "list"),
    ("party_tax_profile", "create"),
    ("party_tax_profile", "update"),
    ("party_bank_account", "list"),
    ("party_bank_account", "create"),
    ("party_bank_account", "update"),
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

/// 发送 DELETE 请求（`TestApi` 只提供 GET/POST，DELETE 路径由本辅助覆盖）。
async fn delete_request(router: &Router, path: &str, token: &str) -> (u16, Value) {
    let request = Request::builder()
        .method(Method::DELETE)
        .uri(path)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        )
        .body(Body::empty())
        .expect("DELETE 请求构造失败");
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

/// 准备带权限的测试环境，返回 `(router, api, token, account_id)`。
async fn setup(test_db: &TestDb) -> (Router, TestApi, String, String) {
    database::ensure_indexes(test_db.db()).await.unwrap();
    let account_id = seed_admin_account(test_db.db()).await.unwrap();
    grant_domain_permissions(test_db.db(), &account_id).await;
    let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
    let (router, _) = build_router(test_db).await;
    let api = TestApi::new(router.clone());
    (router, api, token, account_id)
}

/// 创建主体的请求体。
fn create_party_body(party_no: &str, legal_name: &str) -> Value {
    json!({
        "party_no": party_no,
        "legal_name": legal_name,
        "short_name": "示例",
        "unified_credit_code": "91310000MA1BL4KW9X",
        "effective_from": "2026-01-01",
        "effective_to": "2026-12-31",
        "change_reason": "首次建档",
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/parties", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        // 种子账号只有 role/admin/audit_log.list 权限，本域权限未授予 → 403。
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/parties", Some(&token)).await;
        assert_eq!(status, 403, "无 party.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_400").await.unwrap();
        let (_, api, token, _) = setup(&test_db).await;

        let (status, body) = api
            .post(
                "/admin/parties",
                Some(&token),
                Some(json!({
                    "party_no": "  ",
                    "legal_name": "空编号",
                    "effective_from": "2026-01-01",
                    "change_reason": "x",
                })),
            )
            .await;
        assert_eq!(status, 400, "空白 party_no 必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null, "失败时 data 恒为 null");

        let (status, _) = api
            .post("/admin/parties", Some(&token), Some(json!({ "party_no": "P-1" })))
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/parties",
                Some(&token),
                Some(json!({
                    "party_no": "P-1",
                    "legal_name": "x",
                    "effective_from": "2026-01-01",
                    "change_reason": "x",
                    "party_kind": "MARS",
                })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_create_party_with_revision_then_list_and_detail() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_happy").await.unwrap();
        let (_, api, token, _) = setup(&test_db).await;

        let (status, body) = api.get("/admin/parties", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["items"], json!([]), "初始列表为空");
        assert_eq!(data["total"], 0);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);

        let (status, body) = api
            .post(
                "/admin/parties",
                Some(&token),
                Some(create_party_body("P-2026-001", "上海示例科技有限公司")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        assert_eq!(created["party_no"], "P-2026-001");
        assert_eq!(created["unified_credit_code"], "91310000MA1BL4KW9X");
        assert_eq!(created["party_kind"], "enterprise");
        assert_eq!(created["status"], "active");
        assert_eq!(created["version"], 1);
        assert!(!created["id"].as_str().unwrap().is_empty());
        assert!(created["created_at"].as_u64().unwrap() > 0);
        let party_id = created["id"].as_str().unwrap().to_string();

        let (status, body) = api.get(&format!("/admin/parties/{party_id}"), Some(&token)).await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["party_no"], "P-2026-001");
        let revision = &detail["current_revision"];
        assert_eq!(revision["revision_no"], 1);
        assert_eq!(revision["legal_name"], "上海示例科技有限公司");
        assert_eq!(revision["effective_from"], "2026-01-01");

        let (status, body) = api
            .get(&format!("/admin/parties/{party_id}/revisions"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["items"][0]["revision_no"], 1);

        let (status, body) = api.get("/admin/parties", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        let item = &data["items"][0];
        for field in ["id", "party_no", "party_kind", "status", "created_at", "version"] {
            assert!(item.get(field).is_some(), "契约字段 {field} 必须存在: {item}");
        }
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_party_no_returns_409() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_dup").await.unwrap();
        let (_, api, token, _) = setup(&test_db).await;

        let (_, body) = api
            .post(
                "/admin/parties",
                Some(&token),
                Some(create_party_body("P-2026-001", "甲")),
            )
            .await;
        assert_ok_envelope(200, &body);

        let (status, body) = api
            .post(
                "/admin/parties",
                Some(&token),
                Some(create_party_body("P-2026-001", "乙")),
            )
            .await;
        assert_eq!(status, 409, "重复 party_no 唯一索引冲突必须 409: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);
    })
}

#[tokio::test]
#[ignore]
async fn update_party_appends_revision_and_stale_version_returns_409() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_update").await.unwrap();
        let (router, api, token, _) = setup(&test_db).await;

        let (_, body) = api
            .post(
                "/admin/parties",
                Some(&token),
                Some(create_party_body("P-2026-002", "上海示例科技有限公司")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = put_json(
            &router,
            &format!("/admin/parties/{id}"),
            &token,
            json!({
                "version": 1,
                "legal_name": "上海示例科技（更名）",
                "short_name": "示例",
                "effective_from": "2027-01-01",
                "effective_to": "2027-12-31",
                "change_reason": "公司更名",
            }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["version"], 2, "更新成功版本递增");

        let (status, body) = api
            .get(&format!("/admin/parties/{id}/revisions"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2, "每次更新追加一条修订");

        let (status, body) = put_json(
            &router,
            &format!("/admin/parties/{id}"),
            &token,
            json!({
                "version": 1,
                "legal_name": "陈旧版本",
                "effective_from": "2028-01-01",
                "change_reason": "陈旧提交",
            }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本更新必须 409: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn overlapping_revision_window_rolls_back_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_invariant").await.unwrap();
        let (router, api, token, _) = setup(&test_db).await;

        let (_, body) = api
            .post(
                "/admin/parties",
                Some(&token),
                Some(create_party_body("P-2026-003", "上海示例科技有限公司")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        // 新修订窗口与首版（2026-01-01 ~ 2026-12-31）重叠 → 409 且整体回滚。
        let (status, body) = put_json(
            &router,
            &format!("/admin/parties/{id}"),
            &token,
            json!({
                "version": 1,
                "legal_name": "重叠窗口",
                "effective_from": "2026-06-01",
                "effective_to": "2027-05-31",
                "change_reason": "重叠测试",
            }),
        )
        .await;
        assert_eq!(status, 409, "重叠窗口必须 409: {body}");

        let (status, body) = api
            .get(&format!("/admin/parties/{id}/revisions"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["total"], 1,
            "事务回滚后修订链必须保持原状（只有首版）"
        );
        let (status, body) = api.get(&format!("/admin/parties/{id}"), Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["current_revision"]["revision_no"], 1,
            "主体生效指针未被部分推进"
        );
        assert_eq!(body["data"]["version"], 1, "主体版本未被递增");
    })
}

#[tokio::test]
#[ignore]
async fn bank_account_default_is_exclusive_within_party() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_default").await.unwrap();
        let (router, api, token, _) = setup(&test_db).await;

        let (_, body) = api
            .post(
                "/admin/parties",
                Some(&token),
                Some(create_party_body("P-2026-004", "上海示例科技有限公司")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        async fn create_account(
            api: &TestApi,
            id: &str,
            token: &str,
            no: &str,
            account_number: &str,
        ) -> (u16, Value) {
            let path = format!("/admin/parties/{id}/bank-accounts");
            api.post(
                &path,
                Some(token),
                Some(json!({
                    "bank_account_no": no,
                    "account_name": "上海示例科技有限公司",
                    "bank_name": "招商银行",
                    "account_number": account_number,
                    "valid_from": "2026-01-01",
                    "is_default": true,
                })),
            )
            .await
        }

        let (status, body) = create_account(&api, &id, &token, "BA-001", "6225880212345678").await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["is_default"], true);
        let first_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = create_account(&api, &id, &token, "BA-002", "6225880298765432").await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["is_default"], true);
        let second_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .get(&format!("/admin/parties/{id}/bank-accounts"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let items = &body["data"]["items"];
        let default_ids: Vec<&str> = items
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["is_default"] == json!(true))
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            default_ids,
            vec![second_id.as_str()],
            "同一主体只能有一个默认账户"
        );

        // 更新第一个账户为默认 → 独占性转移。
        let (status, body) = put_json(
            &router,
            &format!("/admin/party-bank-accounts/{first_id}"),
            &token,
            json!({ "version": 2, "is_default": true }),
        )
        .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .get(&format!("/admin/parties/{id}/bank-accounts"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let items = &body["data"]["items"];
        let default_ids: Vec<&str> = items
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["is_default"] == json!(true))
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(default_ids, vec![first_id.as_str()], "默认标记跨行事务转移");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_are_validated() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_page").await.unwrap();
        let (_, api, token, _) = setup(&test_db).await;

        for no in ["P-2026-011", "P-2026-012", "P-2026-013"] {
            let mut body_json = create_party_body(no, "示例公司");
            body_json["unified_credit_code"] = Value::Null;
            let (status, body) = api.post("/admin/parties", Some(&token), Some(body_json)).await;
            assert_ok_envelope(status, &body);
        }

        // 边界页：page_size=2 取第 2 页应只剩 1 条。
        let (status, body) = api
            .get(
                "/admin/parties?page=2&page_size=2&sort_by=party_no&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["items"][0]["party_no"], "P-2026-013");

        // 超界页返回空列表但 total 不变。
        let (status, body) = api.get("/admin/parties?page=9&page_size=20", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"], json!([]));

        // 非法排序字段被 Service 白名单拒绝（400）。
        let (status, body) = api
            .get("/admin/parties?sort_by=hacked&sort_dir=asc", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
        assert_eq!(body["success"], false);

        // 非法排序方向同样被拒。
        let (status, _) = api.get("/admin/parties?sort_dir=up", Some(&token)).await;
        assert_eq!(status, 400, "非法排序方向必须 400");
    })
}

#[tokio::test]
#[ignore]
async fn delete_party_hides_from_list_but_keeps_revisions() {
    require_mongo!(async {
        let test_db = TestDb::new("party_api_delete").await.unwrap();
        let (router, api, token, _) = setup(&test_db).await;

        let (_, body) = api
            .post(
                "/admin/parties",
                Some(&token),
                Some(create_party_body("P-2026-005", "上海示例科技有限公司")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = delete_request(&router, &format!("/admin/parties/{id}"), &token).await;
        assert_ok_envelope(status, &body);

        let (status, body) = api.get("/admin/parties", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "软删除后列表不可见");

        let (status, body) = api
            .get(&format!("/admin/parties/{id}/revisions"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1, "历史修订保留");
    })
}
