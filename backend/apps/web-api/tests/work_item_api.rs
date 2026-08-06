//! 域 D03 `work_item` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test work_item_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入本域权限的
//! 直接 `p` 规则（casbin 的 `g(r.sub, p.sub)` 自反匹配，无需改角色）。

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
    ("work_item", "list"),
    ("work_item", "detail"),
    ("work_item", "create"),
    ("work_item", "claim"),
    ("work_item", "defer"),
    ("work_item", "transfer"),
    ("work_item", "complete"),
    ("work_item", "close"),
];

/// 发送 POST 请求（携带 JSON 体）。
async fn post_json(api: &TestApi, path: &str, token: &str, json: Value) -> (u16, Value) {
    api.post(path, Some(token), Some(json)).await
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

/// 构造最小 AppState 并组装完整应用路由。
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

/// 派发一条导入业务确认待办并返回其 ID。
async fn dispatch_item(api: &TestApi, token: &str, object_id: &str) -> String {
    let (status, body) = api
        .post(
            "/admin/work-items",
            Some(token),
            Some(json!({
                "work_item_type": "IMPORT_BUSINESS_CONFIRMATION",
                "business_object_type": "LEGACY_IMPORT_BATCH",
                "business_object_id": object_id,
                "owner_role": "sales",
                "priority": "high",
                "due_at": 1767225600,
                "reason_code": "IMPORT_READY",
                "impact_summary": "待确认导入范围",
                "completion_action": "COMPLETE_IMPORT_BUSINESS_CONFIRMATION",
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore]
async fn happy_path_dispatch_claim_complete_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("work_item_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/work-items", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0);

        let id = dispatch_item(&api, &token, "batch-1").await;
        let (status, body) = api.get(&format!("/admin/work-items/{id}"), Some(&token)).await;
        assert_ok_envelope(status, &body);
        let item = &body["data"];
        assert_eq!(item["work_item_type"], "IMPORT_BUSINESS_CONFIRMATION");
        assert_eq!(item["status"], "UNCLAIMED");
        assert_eq!(item["priority"], "high");
        assert_eq!(item["completion_action"], "COMPLETE_IMPORT_BUSINESS_CONFIRMATION");
        assert_eq!(item["version"], 1);

        let (status, body) = post_json(
            &api,
            &format!("/admin/work-items/{id}/claim"),
            &token,
            json!({ "version": 1 }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "IN_PROGRESS");
        assert_eq!(body["data"]["owner_user_id"], account_id);
        assert_eq!(body["data"]["version"], 2);

        let (status, body) = post_json(
            &api,
            &format!("/admin/work-items/{id}/complete"),
            &token,
            json!({ "version": 2 }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "COMPLETED");
        assert_eq!(body["data"]["completed_by"], account_id);
        assert!(body["data"]["completed_at"].as_u64().unwrap() > 0);

        let (status, body) = api.get("/admin/work-items", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn claim_race_stale_version_returns_409_and_rolls_back_audit() {
    require_mongo!(async {
        let test_db = TestDb::new("work_item_api_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let id = dispatch_item(&api, &token, "batch-409").await;

        let (status, body) = post_json(
            &api,
            &format!("/admin/work-items/{id}/claim"),
            &token,
            json!({ "version": 1 }),
        )
        .await;
        assert_ok_envelope(status, &body);

        // 再次用陈旧版本领取：行内状态已是 IN_PROGRESS，条件更新不命中 → 409。
        let (status, body) = post_json(
            &api,
            &format!("/admin/work-items/{id}/claim"),
            &token,
            json!({ "version": 1 }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本领取必须 409: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 事务不变量：失败的领取把业务写入与审计一起回滚，审计只保留一条领取记录。
        let count = test_db
            .db()
            .collection::<Document>("audit_logs")
            .count_documents(doc! { "action": "work_item.claim" })
            .await
            .unwrap();
        assert_eq!(count, 1, "失败的领取不得留下审计日志");

        // 暂挂 → 转交 → 完成全链路。
        let (status, body) = post_json(
            &api,
            &format!("/admin/work-items/{id}/defer"),
            &token,
            json!({ "version": 2 }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "UNCLAIMED");
        assert_eq!(body["data"]["owner_user_id"], Value::Null);

        let (status, body) = post_json(
            &api,
            &format!("/admin/work-items/{id}/claim"),
            &token,
            json!({ "version": 3 }),
        )
        .await;
        assert_ok_envelope(status, &body);

        let (status, body) = post_json(
            &api,
            &format!("/admin/work-items/{id}/transfer"),
            &token,
            json!({ "version": 4, "owner_role": "finance", "owner_user_id": "finance-1" }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["owner_role"], "finance");
        assert_eq!(body["data"]["owner_user_id"], "finance-1");
    })
}

#[tokio::test]
#[ignore]
async fn close_is_rejected_for_confirmation_tasks_and_requires_reason() {
    require_mongo!(async {
        let test_db = TestDb::new("work_item_api_close").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let id = dispatch_item(&api, &token, "batch-close").await;

        // 确认类任务不允许人工关闭（实体 is_manually_closable 保守默认）→ 422。
        let (status, body) = post_json(
            &api,
            &format!("/admin/work-items/{id}/close"),
            &token,
            json!({ "version": 1, "close_reason_code": "DUPLICATE_TASK" }),
        )
        .await;
        assert_eq!(status, 422, "确认类任务人工关闭必须 422: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, body) = post_json(
            &api,
            &format!("/admin/work-items/{id}/close"),
            &token,
            json!({ "version": 1, "close_reason_code": "  " }),
        )
        .await;
        assert_eq!(status, 400, "空白关闭原因代码必须 400: {body}");

        let (status, body) = api.get(&format!("/admin/work-items/{id}"), Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "UNCLAIMED", "拒绝的关闭不改变任务状态");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("work_item_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/work-items", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("work_item_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/work-items", Some(&token)).await;
        assert_eq!(status, 403, "无 work_item.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("work_item_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/work-items",
                Some(&token),
                Some(json!({
                    "work_item_type": "IMPORT_BUSINESS_CONFIRMATION",
                    "business_object_type": "LEGACY_IMPORT_BATCH",
                    "business_object_id": "batch-1",
                    "priority": "high",
                    "completion_action": "  ",
                })),
            )
            .await;
        assert_eq!(status, 400, "空白完成动作必须 400: {body}");
        assert_eq!(body["success"], false);

        let (status, _) = api
            .post(
                "/admin/work-items",
                Some(&token),
                Some(json!({ "work_item_type": "NOT_A_TYPE" })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");

        let (status, body) =
            post_json(&api, "/admin/work-items/x/claim", &token, json!({ "version": 0 })).await;
        assert_eq!(status, 400, "非法乐观锁版本必须 400: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn dispatch_requires_registered_business_document() {
    require_mongo!(async {
        let test_db = TestDb::new("work_item_api_cross").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // 业务单据类对象未注册 → 404（跨域 D02 校验）。
        let (status, body) = api
            .post(
                "/admin/work-items",
                Some(&token),
                Some(json!({
                    "work_item_type": "IMPORT_BUSINESS_CONFIRMATION",
                    "business_object_type": "sales_order",
                    "business_object_id": "not-registered",
                    "priority": "normal",
                    "completion_action": "COMPLETE_IMPORT_BUSINESS_CONFIRMATION",
                })),
            )
            .await;
        assert_eq!(status, 404, "业务单据类对象未注册必须 404: {body}");

        // 非单据类对象（导入批次）不需要注册，可正常派发。
        let (status, body) = api
            .post(
                "/admin/work-items",
                Some(&token),
                Some(json!({
                    "work_item_type": "IMPORT_BUSINESS_CONFIRMATION",
                    "business_object_type": "LEGACY_IMPORT_BATCH",
                    "business_object_id": "batch-1",
                    "priority": "normal",
                    "completion_action": "COMPLETE_IMPORT_BUSINESS_CONFIRMATION",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_are_enforced() {
    require_mongo!(async {
        let test_db = TestDb::new("work_item_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        for index in 0..3 {
            dispatch_item(&api, &token, &format!("batch-page-{index}")).await;
        }
        let (status, body) = api
            .get("/admin/work-items?page=2&page_size=2", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["total"], 3);

        let (status, body) = api
            .get(
                "/admin/work-items?owner_user_id=me&sort_by=owner_role",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");
    })
}
