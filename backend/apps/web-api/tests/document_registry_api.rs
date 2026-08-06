//! 域 D02 `document_registry` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test document_registry_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入本域权限的
//! 直接 `p` 规则（casbin 的 `g(r.sub, p.sub)` 自反匹配，无需改角色），
//! 使 happy path 可鉴权通过，同时天然构造 403 用例。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
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
    ("business_document", "list"),
    ("business_document", "create"),
    ("business_document", "detail"),
    ("workflow_action", "list"),
    ("workflow_action", "create"),
    ("document_relation", "list"),
    ("document_relation", "create"),
    ("document_participant", "list"),
    ("document_participant", "create"),
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

/// 注册一张销售单并返回单据 ID。
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

/// 携带幂等键（固定 ID）注册一张销售单并返回单据 ID。
async fn register_order_with_id(api: &TestApi, token: &str, document_no: &str, id: &str) -> String {
    let (status, body) = api
        .post(
            "/admin/business-documents",
            Some(token),
            Some(json!({
                "id": id,
                "document_type": "sales_order",
                "document_no": document_no,
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore]
async fn happy_path_register_list_detail_and_workflow_actions() {
    require_mongo!(async {
        let test_db = TestDb::new("doc_reg_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/business-documents", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["items"], json!([]), "初始列表为空");
        assert_eq!(data["total"], 0);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);

        let (status, body) = api
            .post(
                "/admin/business-documents",
                Some(&token),
                Some(json!({ "document_type": "sales_order", "document_no": "SO-2025-001" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        assert_eq!(created["document_type"], "sales_order");
        assert_eq!(created["document_no"], "SO-2025-001");
        assert_eq!(created["version"], 1);
        assert_eq!(created["formalized_at"], Value::Null);
        let id = created["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .get(&format!("/admin/business-documents/{id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["document_no"], "SO-2025-001");

        let (status, body) = api
            .post(
                "/admin/workflow-actions",
                Some(&token),
                Some(json!({
                    "document_id": id,
                    "action_type": "submit",
                    "from_status": "DRAFT",
                    "to_status": "PENDING_REVIEW",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let action = &body["data"];
        assert_eq!(action["action_type"], "submit");
        assert_eq!(action["from_status"], "DRAFT");
        assert_eq!(action["actor_id"], account_id);
        assert_eq!(action["actor_role"], "admin");
        assert!(action["created_at"].as_u64().unwrap() > 0);

        let (status, body) = api
            .get(&format!("/admin/workflow-actions?document_id={id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["action_type"], "submit");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn register_is_idempotent_and_relations_are_atomic_visible() {
    require_mongo!(async {
        let test_db = TestDb::new("doc_reg_api_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // 携带幂等键注册：同身份同 ID 幂等命中返回已存在行，不产生第二条注册行。
        let first = register_order_with_id(&api, &token, "SO-IDEM-001", "doc-idem-001").await;
        let second = register_order_with_id(&api, &token, "SO-IDEM-001", "doc-idem-001").await;
        assert_eq!(first, second, "同身份同 ID 幂等命中返回已存在行");
        let (status, body) = api.get("/admin/business-documents", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1, "重复注册不产生第二条注册行");

        // 同身份不同 ID 仍然冲突（409）。
        let (status, body) = api
            .post(
                "/admin/business-documents",
                Some(&token),
                Some(json!({
                    "id": "doc-idem-other",
                    "document_type": "sales_order",
                    "document_no": "SO-IDEM-001",
                })),
            )
            .await;
        assert_eq!(status, 409, "同身份不同 ID 必须 409: {body}");

        let order_a = register_order(&api, &token, "SO-IDEM-002").await;
        let (status, body) = api
            .post(
                "/admin/document-relations",
                Some(&token),
                Some(json!({
                    "from_document_id": order_a,
                    "to_document_id": first,
                    "relation_type": "CHANGES",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["relation_type"], "CHANGES");

        let (status, body) = api
            .get(&format!("/admin/documents/{first}/relations"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let relations = body["data"].as_array().unwrap();
        assert_eq!(relations.len(), 1, "入向关系可见");
        assert_eq!(relations[0]["from_document_id"], order_a);
    })
}

#[tokio::test]
#[ignore]
async fn register_rejects_duplicate_identity_with_different_id() {
    require_mongo!(async {
        let test_db = TestDb::new("doc_reg_api_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        register_order(&api, &token, "SO-CONFLICT-001").await;
        let (status, body) = api
            .post(
                "/admin/business-documents",
                Some(&token),
                Some(json!({ "document_type": "sales_order", "document_no": "SO-CONFLICT-001" })),
            )
            .await;
        assert_eq!(status, 409, "同身份不同 ID 的重复注册必须 409: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("doc_reg_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/business-documents", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("doc_reg_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        // 种子账号只有 role/admin/audit_log.list 权限，本域权限未授予 → 403。
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/business-documents", Some(&token)).await;
        assert_eq!(status, 403, "无 business_document.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("doc_reg_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/business-documents",
                Some(&token),
                Some(json!({ "document_type": "sales_order", "document_no": "   " })),
            )
            .await;
        assert_eq!(status, 400, "空白编号必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, _) = api
            .post(
                "/admin/business-documents",
                Some(&token),
                Some(json!({ "document_type": "sales_order" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/business-documents",
                Some(&token),
                Some(json!({ "document_type": "mars_rocket", "document_no": "X-1" })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn workflow_action_requires_registered_document_and_participants_are_recorded() {
    require_mongo!(async {
        let test_db = TestDb::new("doc_reg_api_flow").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/workflow-actions",
                Some(&token),
                Some(json!({
                    "document_id": "missing-doc",
                    "action_type": "submit",
                    "from_status": "DRAFT",
                    "to_status": "PENDING_REVIEW",
                })),
            )
            .await;
        assert_eq!(status, 404, "未注册单据追加动作必须 404: {body}");
        assert_eq!(body["success"], false);
        let id = register_order(&api, &token, "SO-PART-001").await;

        let (status, body) = api
            .post(
                "/admin/document-participants",
                Some(&token),
                Some(json!({
                    "document_id": id,
                    "participant_role": "owner_sales",
                    "participant_user_id": "sales-1",
                    "participant_name": "张三",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["participant_role"], "owner_sales");
        assert_eq!(body["data"]["recorded_by"], account_id);

        let (status, body) = api
            .get("/admin/document-participants?user_id=sales-1", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"][0]["document_id"], id);
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_are_enforced() {
    require_mongo!(async {
        let test_db = TestDb::new("doc_reg_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        for index in 0..3 {
            register_order(&api, &token, &format!("SO-PAGE-{index}")).await;
        }
        let (status, body) = api
            .get("/admin/business-documents?page=2&page_size=2", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["page"], 2);
        assert_eq!(body["data"]["page_size"], 2);

        let (status, body) = api
            .get("/admin/business-documents?sort_by=document_no", Some(&token))
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");
        assert_eq!(body["success"], false);
    })
}
