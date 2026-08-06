//! 域 D04 `bulk_job` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test bulk_job_api -- --include-ignored`。
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
    ("bulk_selection_snapshot", "list"),
    ("bulk_selection_snapshot", "create"),
    ("bulk_selection_snapshot", "confirm"),
    ("bulk_selection_snapshot", "expire"),
    ("bulk_selection_item", "list"),
    ("background_job", "list"),
    ("background_job", "detail"),
    ("background_job", "create"),
    ("background_job", "cancel"),
    ("background_job_item", "list"),
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

/// 创建选择快照并返回快照 ID。
async fn create_snapshot(api: &TestApi, token: &str) -> String {
    let (status, body) = api
        .post(
            "/admin/bulk-selection-snapshots",
            Some(token),
            Some(json!({
                "selection_type": "export",
                "data_cutoff_at": 1767225600,
                "expires_at": 1767312000,
                "items": [
                    { "object_type": "legacy_import_batch", "object_id": "batch-1" },
                    { "object_type": "legacy_import_batch", "object_id": "batch-2" },
                ],
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore]
async fn happy_path_snapshot_confirm_items_and_job_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("bulk_job_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let snapshot_id = create_snapshot(&api, &token).await;
        let (status, body) = api
            .get(
                &format!("/admin/bulk-selection-snapshots/{snapshot_id}/items?page=1&page_size=10"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2);
        assert_eq!(body["data"]["items"][0]["object_type"], "legacy_import_batch");
        assert_eq!(body["data"]["items"][0]["selection_snapshot_id"], snapshot_id);

        let (status, body) = api
            .post(
                &format!("/admin/bulk-selection-snapshots/{snapshot_id}/confirm"),
                Some(&token),
                Some(json!({ "version": 1 })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "confirmed");
        assert_eq!(body["data"]["version"], 2);

        let (status, body) = api
            .post(
                "/admin/background-jobs",
                Some(&token),
                Some(json!({
                    "job_no": "JOB-2026-001",
                    "job_type": "export",
                    "domain_job_type": "legacy_import_batch",
                    "domain_job_id": "batch-1",
                    "selection_snapshot_id": snapshot_id,
                    "request_id": "req-export-1",
                    "total_count": 2,
                    "items": [
                        {},
                        {},
                    ],
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let job = &body["data"];
        assert_eq!(job["job_no"], "JOB-2026-001");
        assert_eq!(job["status"], "pending");
        assert_eq!(job["requested_by"], account_id);
        assert_eq!(job["total_count"], 2);
        assert_eq!(job["processed_count"], 0);
        let job_id = job["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .get(&format!("/admin/background-jobs/{job_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["job_type"], "export");

        let (status, body) = api
            .get(
                &format!("/admin/background-jobs/{job_id}/items?page=1&page_size=10"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2);
        assert_eq!(body["data"]["items"][0]["item_no"], 1);

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn background_job_create_is_idempotent_by_request_id() {
    require_mongo!(async {
        let test_db = TestDb::new("bulk_job_api_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let body = json!({
            "job_no": "JOB-IDEM-001",
            "job_type": "import",
            "request_id": "req-idem-1",
            "total_count": 1,
            "items": [{ "object_type": "legacy_import_row", "object_id": "row-1" }],
        });
        let (status, first) = api
            .post("/admin/background-jobs", Some(&token), Some(body.clone()))
            .await;
        assert_ok_envelope(status, &first);
        let (status, second) = api.post("/admin/background-jobs", Some(&token), Some(body)).await;
        assert_ok_envelope(status, &second);
        assert_eq!(
            first["data"]["id"], second["data"]["id"],
            "同一 request_id 幂等命中返回既有任务"
        );

        let (status, body) = api.get("/admin/background-jobs", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1, "重复提交不产生第二条任务");
    })
}

#[tokio::test]
#[ignore]
async fn snapshot_and_job_are_written_atomically_or_not_at_all() {
    require_mongo!(async {
        let test_db = TestDb::new("bulk_job_api_atomic").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // 冻结目标命中业务单据目录但单据未注册 → 404，快照与逐项全部不可见。
        let (status, body) = api
            .post(
                "/admin/bulk-selection-snapshots",
                Some(&token),
                Some(json!({
                    "selection_type": "export",
                    "data_cutoff_at": 1767225600,
                    "expires_at": 1767312000,
                    "items": [{ "object_type": "sales_order", "object_id": "not-registered" }],
                })),
            )
            .await;
        assert_eq!(status, 404, "业务单据类目标未注册必须 404: {body}");
        let snapshots = test_db
            .db()
            .collection::<Document>("bulk_selection_snapshots")
            .count_documents(doc! {})
            .await
            .unwrap();
        let items = test_db
            .db()
            .collection::<Document>("bulk_selection_items")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(snapshots, 0, "失败后快照不可见");
        assert_eq!(items, 0, "失败后冻结目标不可见");

        // 逐项对象类型/ID 不成对被实体拒绝 → 400，任务与逐项全部不可见。
        let (status, body) = api
            .post(
                "/admin/background-jobs",
                Some(&token),
                Some(json!({
                    "job_no": "JOB-ATOMIC-001",
                    "job_type": "import",
                    "request_id": "req-atomic-1",
                    "total_count": 1,
                    "items": [{ "object_type": "legacy_import_row" }],
                })),
            )
            .await;
        assert_eq!(status, 422, "逐项对象类型/ID不成对必须 422: {body}");
        let jobs = test_db
            .db()
            .collection::<Document>("background_jobs")
            .count_documents(doc! {})
            .await
            .unwrap();
        let job_items = test_db
            .db()
            .collection::<Document>("background_job_items")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(jobs, 0, "失败后任务不可见");
        assert_eq!(job_items, 0, "失败后逐项结果不可见");
    })
}

#[tokio::test]
#[ignore]
async fn snapshot_expire_and_job_cancel_use_optimistic_lock() {
    require_mongo!(async {
        let test_db = TestDb::new("bulk_job_api_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let snapshot_id = create_snapshot(&api, &token).await;
        let (status, body) = api
            .post(
                &format!("/admin/bulk-selection-snapshots/{snapshot_id}/confirm"),
                Some(&token),
                Some(json!({ "version": 1 })),
            )
            .await;
        assert_ok_envelope(status, &body);

        let (status, body) = api
            .post(
                &format!("/admin/bulk-selection-snapshots/{snapshot_id}/expire"),
                Some(&token),
                Some(json!({ "version": 1 })),
            )
            .await;
        assert_eq!(status, 409, "陈旧版本失效必须 409: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .post(
                &format!("/admin/bulk-selection-snapshots/{snapshot_id}/expire"),
                Some(&token),
                Some(json!({ "version": 2 })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "expired");

        let (status, body) = api
            .post(
                "/admin/background-jobs",
                Some(&token),
                Some(json!({
                    "job_no": "JOB-CANCEL-001",
                    "job_type": "sync",
                    "request_id": "req-cancel-1",
                    "total_count": 1,
                    "items": [{}],
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let job_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                &format!("/admin/background-jobs/{job_id}/cancel"),
                Some(&token),
                Some(json!({ "version": 1 })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "cancelled");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("bulk_job_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/background-jobs", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("bulk_job_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/background-jobs", Some(&token)).await;
        assert_eq!(status, 403, "无 background_job.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("bulk_job_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/bulk-selection-snapshots",
                Some(&token),
                Some(json!({
                    "selection_type": "export",
                    "data_cutoff_at": 1767225600,
                    "expires_at": 1767312000,
                    "items": [],
                })),
            )
            .await;
        assert_eq!(status, 400, "空冻结目标必须 400: {body}");
        assert_eq!(body["success"], false);

        let (status, _) = api
            .post(
                "/admin/background-jobs",
                Some(&token),
                Some(json!({ "job_type": "teleport" })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");

        let (status, body) = api
            .get("/admin/background-jobs?sort_by=job_no", Some(&token))
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");
    })
}
