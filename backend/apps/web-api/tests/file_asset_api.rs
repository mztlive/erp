//! 域 D05 `file_asset` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test file_asset_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入本域权限的
//! 直接 `p` 规则（casbin 的 `g(r.sub, p.sub)` 自反匹配，无需改角色）。

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
    ("file_asset", "list"),
    ("file_asset", "detail"),
    ("file_asset", "create"),
    ("file_asset", "update"),
    ("file_asset", "delete"),
    ("document_attachment", "list"),
    ("document_attachment", "create"),
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

/// 登记一张销售单并返回单据 ID。
async fn register_order(api: &TestApi, token: &str, document_no: &str) -> String {
    let (status, body) = api
        .post(
            "/admin/business-documents",
            Some(token),
            Some(json!({ "document_type": "sales_order", "document_no": document_no })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"]["id"].as_str().unwrap().to_string()
}

/// 登记一个待扫描文件资产并返回其 ID。
async fn register_asset(api: &TestApi, token: &str) -> String {
    let (status, body) = api
        .post(
            "/admin/file-assets/register",
            Some(token),
            Some(json!({
                "storage_object_key": format!("obj/2026/08/{}", uuid::Uuid::new_v4()),
                "file_name": "导入清单.xlsx",
                "content_type": "application/vnd.ms-excel",
                "byte_size": 2048,
                "content_hmac": "a".repeat(64),
                "sensitivity_class": "sensitive",
                "retention_class": "thirty_days",
                "expires_at": 1767312000,
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore]
async fn happy_path_register_scan_attach_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("file_asset_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (status, body) = api.get("/admin/file-assets", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0);

        let asset_id = register_asset(&api, &token).await;
        let (status, body) = api
            .get(&format!("/admin/file-assets/{asset_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let asset = &body["data"];
        assert_eq!(asset["security_scan_status"], "pending");
        assert_eq!(asset["sensitivity_class"], "sensitive");
        assert_eq!(asset["retention_class"], "thirty_days");
        assert_eq!(asset["version"], 1);
        assert!(asset["storage_object_key"].as_str().unwrap().starts_with("obj/"));

        // 待扫描资产不能关联正式业务对象 → 422。
        let order_id = register_order(&api, &token, "SO-FILE-001").await;
        let (status, body) = api
            .post(
                "/admin/document-attachments",
                Some(&token),
                Some(json!({
                    "document_id": order_id,
                    "file_asset_id": asset_id,
                    "usage": "attachment",
                })),
            )
            .await;
        assert_eq!(status, 422, "未通过安全检查的资产不可关联: {body}");
        assert_eq!(body["success"], false);

        // 安全检查通过后可关联。
        let (status, body) = put_json(
            &router,
            &format!("/admin/file-assets/{asset_id}/scan-result"),
            &token,
            json!({ "version": 1, "security_scan_status": "passed" }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["security_scan_status"], "passed");

        let (status, body) = api
            .post(
                "/admin/document-attachments",
                Some(&token),
                Some(json!({
                    "document_id": order_id,
                    "file_asset_id": asset_id,
                    "usage": "attachment",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["usage"], "attachment");
        assert_eq!(body["data"]["created_by"], account_id);

        let (status, body) = api
            .get(&format!("/admin/documents/{order_id}/attachments"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"][0]["file_asset_id"], asset_id);

        let (status, body) = api.get("/admin/file-assets", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        let row = &body["data"]["items"][0];
        for field in [
            "id",
            "file_name",
            "content_type",
            "byte_size",
            "security_scan_status",
        ] {
            assert!(row.get(field).is_some(), "列表契约字段 {field} 必须存在");
        }
        assert!(
            row.get("storage_object_key").is_none(),
            "列表不得暴露敏感对象存储键"
        );

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn upload_endpoint_saves_under_upload_path_and_registers_asset() {
    require_mongo!(async {
        let test_db = TestDb::new("file_asset_api_upload").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;

        let boundary = "c-g1-test-boundary";
        let body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"sensitivity_class\"\r\n\r\n\
             general\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"retention_class\"\r\n\r\n\
             seven_days\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"expires_at\"\r\n\r\n\
             1767312000\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"result.csv\"\r\n\
             Content-Type: text/csv\r\n\r\n\
             a,b,c\r\n\
             --{boundary}--\r\n"
        );
        let request = Request::builder()
            .method(Method::POST)
            .uri("/admin/file-assets/upload")
            .header(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
            )
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .expect("上传请求构造失败");
        let response = router.clone().oneshot(request).await.expect("路由调用失败");
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("响应体读取失败");
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        assert_ok_envelope(status, &value);
        let asset = &value["data"];
        assert_eq!(asset["file_name"], "result.csv");
        assert_eq!(asset["retention_class"], "seven_days");
        assert_eq!(asset["content_hmac"].as_str().unwrap().len(), 64);
        let object_key = asset["storage_object_key"].as_str().unwrap();

        let stored = upload_path.join(object_key);
        assert!(stored.exists(), "上传文件必须落在配置 upload_path 内: {stored:?}");
        assert_eq!(
            tokio::fs::read_to_string(&stored).await.unwrap(),
            "a,b,c",
            "文件内容与上传一致"
        );
        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn destroy_is_optimistic_and_blocks_reuse() {
    require_mongo!(async {
        let test_db = TestDb::new("file_asset_api_destroy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let asset_id = register_asset(&api, &token).await;

        let (status, body) = api
            .post(
                &format!("/admin/file-assets/{asset_id}/destroy"),
                Some(&token),
                Some(json!({ "version": 1 })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert!(body["data"]["destroyed_at"].as_u64().unwrap() > 0);

        let (status, body) = api
            .post(
                &format!("/admin/file-assets/{asset_id}/destroy"),
                Some(&token),
                Some(json!({ "version": 1 })),
            )
            .await;
        assert_eq!(status, 409, "陈旧版本销毁必须 409: {body}");

        // 已销毁资产不可再用于业务关联（即使扫描通过）。
        let order_id = register_order(&api, &token, "SO-FILE-002").await;
        let (status, body) = put_json(
            &router,
            &format!("/admin/file-assets/{asset_id}/scan-result"),
            &token,
            json!({ "version": 2, "security_scan_status": "passed" }),
        )
        .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .post(
                "/admin/document-attachments",
                Some(&token),
                Some(json!({
                    "document_id": order_id,
                    "file_asset_id": asset_id,
                    "usage": "attachment",
                })),
            )
            .await;
        assert_eq!(status, 422, "已销毁资产不可关联: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("file_asset_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/file-assets", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("file_asset_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/file-assets", Some(&token)).await;
        assert_eq!(status, 403, "无 file_asset.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("file_asset_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/file-assets/register",
                Some(&token),
                Some(json!({
                    "storage_object_key": "obj/x",
                    "file_name": " ",
                    "content_type": "text/plain",
                    "byte_size": 1,
                    "content_hmac": "a".repeat(64),
                    "sensitivity_class": "general",
                    "retention_class": "thirty_days",
                    "expires_at": 1767312000,
                })),
            )
            .await;
        assert_eq!(status, 400, "空白文件名必须 400: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .post(
                "/admin/file-assets/register",
                Some(&token),
                Some(json!({
                    "storage_object_key": "obj/x",
                    "file_name": "x.csv",
                    "content_type": "text/plain",
                    "byte_size": 1,
                    "content_hmac": "not-a-hex",
                    "sensitivity_class": "general",
                    "retention_class": "long_term",
                })),
            )
            .await;
        assert_eq!(status, 400, "非法内容指纹必须 400: {body}");

        let (status, body) = api
            .post(
                "/admin/file-assets/register",
                Some(&token),
                Some(json!({
                    "storage_object_key": "obj/x",
                    "file_name": "x.csv",
                    "content_type": "text/plain",
                    "byte_size": 1,
                    "content_hmac": "a".repeat(64),
                    "sensitivity_class": "general",
                    "retention_class": "thirty_days",
                })),
            )
            .await;
        assert_eq!(status, 400, "非长期保留策略缺到期时间必须 400: {body}");

        let (status, _) = api
            .post(
                "/admin/document-attachments",
                Some(&token),
                Some(json!({ "usage": "teleport" })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_are_enforced() {
    require_mongo!(async {
        let test_db = TestDb::new("file_asset_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        for _ in 0..3 {
            register_asset(&api, &token).await;
        }
        let (status, body) = api
            .get("/admin/file-assets?page=2&page_size=2", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["page"], 2);
        assert_eq!(body["data"]["page_size"], 2);

        let (status, body) = api
            .get("/admin/file-assets?sort_by=file_name", Some(&token))
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");
    })
}
