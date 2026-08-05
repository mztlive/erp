//! 域 D01 `source_registry` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test source_registry_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入
//! `source_system`/`external_identity_map` 的直接 `p` 规则
//! （casbin 的 `g(r.sub, p.sub)` 自反匹配，无需改角色），
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
    ("source_system", "list"),
    ("source_system", "create"),
    ("source_system", "update"),
    ("external_identity_map", "list"),
    ("external_identity_map", "create"),
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

#[tokio::test]
#[ignore]
async fn happy_path_create_source_system_then_list_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/source-systems", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["items"], json!([]), "初始列表为空");
        assert_eq!(data["total"], 0);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);

        let (status, body) = api
            .post(
                "/admin/source-systems",
                Some(&token),
                Some(json!({ "code": "ERP", "name": "ERP 系统", "system_type": "ERP" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        assert_eq!(created["code"], "ERP");
        assert_eq!(created["name"], "ERP 系统");
        assert_eq!(created["system_type"], "ERP");
        assert_eq!(created["status"], "active");
        assert_eq!(created["version"], 1);
        assert!(!created["id"].as_str().unwrap().is_empty());
        assert!(created["created_at"].as_u64().unwrap() > 0);

        let (status, body) = api.get("/admin/source-systems", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);
        let item = &data["items"][0];
        for field in ["id", "code", "name", "system_type", "status", "created_at"] {
            assert!(item.get(field).is_some(), "契约字段 {field} 必须存在: {item}");
        }
        assert_eq!(item["code"], "ERP");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn create_external_identity_map_writes_map_and_target_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_api_map").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/source-systems",
                Some(&token),
                Some(json!({ "code": "MALL", "name": "目标商城", "system_type": "MALL" })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let source_system_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/external-identity-maps",
                Some(&token),
                Some(json!({
                    "source_system_id": source_system_id,
                    "object_type": "sales_order",
                    "external_id": "SO-2025-001",
                    "internal_object_type": "sales_order",
                    "internal_object_id": "SO-2025-001",
                    "relation_role": "PRIMARY",
                    "valid_from": 1754438400,
                    "valid_to": 1754524800,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let map = &body["data"];
        assert_eq!(map["external_id"], "SO-2025-001");
        assert_eq!(map["object_type"], "sales_order");
        assert_eq!(map["mapping_status"], "pending");
        assert_eq!(map["source_system_id"], source_system_id);

        let (status, body) = api
            .get(
                &format!("/admin/external-identity-maps?source_system_id={source_system_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1, "按来源系统筛选应命中新建映射");
        assert_eq!(data["items"][0]["external_id"], "SO-2025-001");
        assert_eq!(data["items"][0]["mapping_status"], "pending");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/source-systems", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        // 种子账号只有 role/admin/audit_log.list 权限，本域权限未授予 → 403。
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/source-systems", Some(&token)).await;
        assert_eq!(status, 403, "无 source_system.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/source-systems",
                Some(&token),
                Some(json!({ "code": "  ", "name": "空代码", "system_type": "ERP" })),
            )
            .await;
        assert_eq!(status, 400, "空白 code 必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null, "失败时 data 恒为 null");

        // serde 反序列化失败（缺字段/非法枚举）走 axum 0.8 `Json` 提取器的内建
        // 拒绝路径，恒为 422（axum 0.8 默认；自定义拒绝处理器需要改冻结的
        // routes/errors，域内不做）。与 DTO 校验的 400 信封不同：此时响应体
        // 不是 ApiResponse 信封，前端按非 2xx 处理。
        let (status, _) = api
            .post(
                "/admin/source-systems",
                Some(&token),
                Some(json!({ "code": "ERP" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/source-systems",
                Some(&token),
                Some(json!({ "code": "ERP", "name": "x", "system_type": "MARS" })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_update_returns_409() {
    require_mongo!(async {
        let test_db = TestDb::new("src_reg_api_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (_, body) = api
            .post(
                "/admin/source-systems",
                Some(&token),
                Some(json!({ "code": "ERP", "name": "ERP 系统", "system_type": "ERP" })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/source-systems",
                Some(&token),
                Some(json!({ "code": "ERP", "name": "重复", "system_type": "ERP" })),
            )
            .await;
        assert_eq!(status, 409, "重复 code 唯一索引冲突必须 409: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = put_json(
            &router,
            &format!("/admin/source-systems/{id}"),
            &token,
            json!({ "version": 1, "name": "第一次更新" }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["version"], 2, "更新成功版本递增");
        assert_eq!(body["data"]["name"], "第一次更新");

        let (status, body) = put_json(
            &router,
            &format!("/admin/source-systems/{id}"),
            &token,
            json!({ "version": 1, "name": "陈旧版本更新" }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本更新必须 409: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);
    })
}
