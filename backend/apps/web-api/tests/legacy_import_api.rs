//! 域 D22 `legacy_import` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test legacy_import_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入本域资源的直接
//! `p` 规则（casbin 的 `g(r.sub, p.sub)` 自反匹配），使 happy path 可鉴权通过，
//! 同时天然构造 403 用例。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use database::PartyExt;
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
    ("legacy_import_batch", "list"),
    ("legacy_import_batch", "create"),
    ("legacy_import_batch", "detail"),
    ("legacy_import_batch", "apply"),
    ("legacy_import_row", "list"),
    ("legacy_import_confirmation", "list"),
    ("legacy_import_confirmation", "create"),
    ("legacy_import_confirmation", "decide"),
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

/// 创建测试批次请求体（默认一条 CARD_SALES_ORDER 行）。
fn create_batch_json(batch_no: &str, source_object_set: &str, rows: Value) -> Value {
    json!({
        "batch_no": batch_no,
        "source_system_id": "sys-mall",
        "source_object_set": source_object_set,
        "baseline_date": "2026-01-01",
        "import_rule_version": "v1",
        "source_file_hmac": format!("hmac-{batch_no}"),
        "rows": rows,
    })
}

/// 创建测试导入行。
fn row_json(source_object_type: &str, source_row_key: &str) -> Value {
    json!({
        "source_object_type": source_object_type,
        "source_row_key": source_row_key,
        "normalized_payload_reference": format!(r#"{{"row":"{source_row_key}"}}"#),
    })
}

/// 走完批次到 `Importing` 的准备链路：创建批次 + 创建确认 + 确认全绿。
/// 返回 (router, api, token, batch_id, confirmation_ids)。
async fn prepare_importing_batch(api: &TestApi, token: &str, batch_no: &str) -> (String, Vec<String>) {
    let (_, body) = api
        .post(
            "/admin/legacy-import-batches",
            Some(token),
            Some(create_batch_json(
                batch_no,
                "CARD_SALES_ORDER",
                json!([
                    row_json("CARD_SALES_ORDER", "row-1"),
                    row_json("CARD_SALES_ORDER", "row-2")
                ]),
            )),
        )
        .await;
    assert_ok_envelope(200, &body);
    let batch_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = api
        .post(
            "/admin/legacy-import-confirmations",
            Some(token),
            Some(json!({
                "batch_id": batch_id,
                "confirmation_scope": "SALES",
                "owner_role": "销售领导",
                "batch_version": 1,
                "trial_version": 1,
                "import_rule_version": "v1",
                "work_item_id": "wi-sales-1",
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let confirmation_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = api
        .post(
            &format!("/admin/legacy-import-confirmations/{confirmation_id}/decide"),
            Some(token),
            Some(json!({ "decision": "CONFIRM_SCOPE", "comment": "试算无误" })),
        )
        .await;
    assert_ok_envelope(status, &body);

    let (_, body) = api
        .get(&format!("/admin/legacy-import-batches/{batch_id}"), Some(token))
        .await;
    assert_eq!(
        body["data"]["status"], "importing",
        "确认全绿后批次进入导入中: {body}"
    );
    (batch_id, vec![confirmation_id])
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/legacy-import-batches", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/legacy-import-batches", Some(&token)).await;
        assert_eq!(status, 403, "无 legacy_import_batch.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(create_batch_json(
                    "  ",
                    "CARD_SALES_ORDER",
                    json!([row_json("CARD_SALES_ORDER", "r1")]),
                )),
            )
            .await;
        assert_eq!(status, 400, "空白批次号必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, _) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(create_batch_json("IMP-400-2", "CARD_SALES_ORDER", json!([]))),
            )
            .await;
        assert_eq!(status, 400, "空行列表必须 400");

        let (status, _) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(json!({ "batch_no": "IMP-400-3" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_create_confirm_decide_apply_and_list() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(create_batch_json(
                    "IMP-2026-001",
                    "CARD_SALES_ORDER",
                    json!([
                        row_json("CARD_SALES_ORDER", "row-1"),
                        row_json("CARD_SALES_ORDER", "row-2")
                    ]),
                )),
            )
            .await;
        assert_ok_envelope(200, &body);
        let batch = &body["data"];
        for field in [
            "id",
            "batch_no",
            "source_system_id",
            "source_object_set",
            "baseline_date",
            "import_rule_version",
            "status",
            "total_rows",
            "success_rows",
            "failed_rows",
            "background_job_id",
            "version",
            "created_at",
        ] {
            assert!(batch.get(field).is_some(), "契约字段 {field} 必须存在: {batch}");
        }
        assert_eq!(batch["batch_no"], "IMP-2026-001");
        assert_eq!(batch["status"], "pending_validation");
        assert_eq!(batch["total_rows"], 2);
        assert!(
            !batch["background_job_id"].as_str().unwrap().is_empty(),
            "创建批次必须登记后台任务"
        );
        let batch_id = batch["id"].as_str().unwrap().to_string();

        let (status, body) = api.get("/admin/legacy-import-batches", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);
        assert_eq!(data["items"][0]["batch_no"], "IMP-2026-001");

        let (status, body) = api
            .get(
                &format!("/admin/legacy-import-batches/{batch_id}/rows"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2, "批次含两行导入行");
        assert_eq!(body["data"]["items"][0]["parse_status"], "pending_parse");
        assert_eq!(body["data"]["items"][0]["import_status"], "pending_import");

        let (status, body) = api
            .get(
                &format!("/admin/legacy-import-confirmations?batch_id={batch_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "创建确认前无确认事实");

        let (batch_id, _) = prepare_importing_batch(&api, &token, "IMP-2026-001").await;

        let (_, body) = api
            .get(
                &format!("/admin/legacy-import-batches/{batch_id}/rows"),
                Some(&token),
            )
            .await;
        let row_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-batches/{batch_id}/apply"),
                Some(&token),
                Some(json!({
                    "results": [
                        {
                            "row_id": row_id,
                            "outcome": "imported",
                            "external_identity_map_id": "eim-1",
                            "target_document_id": "SO-100",
                            "target_object_reference": "sales/so-100"
                        }
                    ]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "importing", "未全部处理保持导入中");
        assert_eq!(body["data"]["success_rows"], 1);
        assert_eq!(body["data"]["failed_rows"], 0);

        let (_, body) = api
            .get(
                &format!("/admin/legacy-import-batches/{batch_id}/rows"),
                Some(&token),
            )
            .await;
        let pending_rows: Vec<String> = body["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["import_status"] == "pending_import")
            .map(|item| item["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(pending_rows.len(), 1, "另一行保持待导入");
        let results: Vec<Value> = pending_rows
            .iter()
            .map(|id| {
                json!({
                    "row_id": id,
                    "outcome": "imported",
                    "external_identity_map_id": "eim-2",
                    "target_document_id": "SO-101"
                })
            })
            .collect();
        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-batches/{batch_id}/apply"),
                Some(&token),
                Some(json!({ "results": results })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "completed", "全部成功导入后批次完成");
        assert_eq!(body["data"]["success_rows"], 2);

        let (status, body) = api
            .get(&format!("/admin/legacy-import-batches/{batch_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert!(
            !body["data"]["background_job_id"].as_str().unwrap().is_empty(),
            "详情含后台任务关联"
        );
    })
}

#[tokio::test]
#[ignore]
async fn create_batch_rolls_back_atomically_on_row_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_atomic").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let duplicated_rows = json!([
            row_json("CARD_SALES_ORDER", "row-1"),
            row_json("CARD_SALES_ORDER", "row-1"),
        ]);
        let (status, body) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(create_batch_json(
                    "IMP-ATOMIC-1",
                    "CARD_SALES_ORDER",
                    duplicated_rows,
                )),
            )
            .await;
        assert_eq!(status, 409, "行唯一索引冲突必须 409: {body}");
        assert_eq!(body["success"], false);

        let (_, body) = api.get("/admin/legacy-import-batches", Some(&token)).await;
        assert_eq!(body["data"]["total"], 0, "事务回滚后批次不可见");
        let (_, body) = api
            .get(
                "/admin/legacy-import-confirmations?batch_id=IMP-ATOMIC-1",
                Some(&token),
            )
            .await;
        assert_eq!(body["data"]["total"], 0);
        let background_jobs = test_db
            .db()
            .collection::<Document>("background_jobs")
            .count_documents(doc! { "request_id": "IMP-ATOMIC-1" })
            .await
            .unwrap();
        assert_eq!(background_jobs, 0, "后台任务随事务回滚不可见");
        let audit_logs = test_db
            .db()
            .collection::<Document>("audit_logs")
            .count_documents(
                doc! { "resource_type": "legacy_import_batch", "action": "legacy_import_batch.create" },
            )
            .await
            .unwrap();
        assert_eq!(audit_logs, 0, "审计日志随事务回滚不可见");
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_batch_no_returns_existing_idempotently() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(create_batch_json(
                    "IMP-IDEM-1",
                    "CARD_SALES_ORDER",
                    json!([row_json("CARD_SALES_ORDER", "row-1")]),
                )),
            )
            .await;
        assert_ok_envelope(200, &body);
        let first_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(create_batch_json(
                    "IMP-IDEM-1",
                    "CARD_SALES_ORDER",
                    json!([row_json("CARD_SALES_ORDER", "row-1")]),
                )),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["id"].as_str().unwrap(),
            first_id,
            "重复批次号按幂等返回既有批次"
        );

        let background_jobs = test_db
            .db()
            .collection::<Document>("background_jobs")
            .count_documents(doc! { "request_id": "IMP-IDEM-1" })
            .await
            .unwrap();
        assert_eq!(background_jobs, 1, "重复提交不产生重复后台任务");
        let batches = test_db
            .db()
            .collection::<Document>("legacy_import_batches")
            .count_documents(doc! { "batch_no": "IMP-IDEM-1" })
            .await
            .unwrap();
        assert_eq!(batches, 1, "重复提交不产生重复批次");
    })
}

#[tokio::test]
#[ignore]
async fn decide_confirmation_is_idempotent_and_rejects_conflicting_decision() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_decide").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(create_batch_json(
                    "IMP-DECIDE-1",
                    "CARD_SALES_ORDER",
                    json!([row_json("CARD_SALES_ORDER", "row-1")]),
                )),
            )
            .await;
        let batch_id = body["data"]["id"].as_str().unwrap().to_string();
        let (_, body) = api
            .post(
                "/admin/legacy-import-confirmations",
                Some(&token),
                Some(json!({
                    "batch_id": batch_id,
                    "confirmation_scope": "FINANCE",
                    "owner_role": "财务领导",
                    "batch_version": 1,
                    "trial_version": 1,
                    "import_rule_version": "v1",
                    "work_item_id": "wi-fin-1",
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let confirmation_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-confirmations/{confirmation_id}/decide"),
                Some(&token),
                Some(json!({ "decision": "RETURN_FOR_FIX" })),
            )
            .await;
        assert_eq!(status, 422, "退回必须携带原因代码（业务规则）: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-confirmations/{confirmation_id}/decide"),
                Some(&token),
                Some(json!({ "decision": "RETURN_FOR_FIX", "reason_code": "CUSTOMER_DATA_MISSING" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "REJECTED");
        assert_eq!(body["data"]["decision"], "RETURN_FOR_FIX");

        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-confirmations/{confirmation_id}/decide"),
                Some(&token),
                Some(json!({ "decision": "RETURN_FOR_FIX", "reason_code": "CUSTOMER_DATA_MISSING" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "REJECTED", "相同决策重复提交按幂等返回");

        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-confirmations/{confirmation_id}/decide"),
                Some(&token),
                Some(json!({ "decision": "CONFIRM_SCOPE" })),
            )
            .await;
        assert_eq!(status, 409, "已退回后改确认必须 409: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn apply_rejects_batch_not_in_importing_stage() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_stage").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(create_batch_json(
                    "IMP-STAGE-1",
                    "CARD_SALES_ORDER",
                    json!([row_json("CARD_SALES_ORDER", "row-1")]),
                )),
            )
            .await;
        let batch_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-batches/{batch_id}/apply"),
                Some(&token),
                Some(json!({ "results": [] })),
            )
            .await;
        assert_eq!(status, 400, "空结果列表必须 400: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-batches/{batch_id}/apply"),
                Some(&token),
                Some(json!({
                    "results": [
                        {
                            "row_id": "row-missing",
                            "outcome": "failed",
                            "error_code": "X"
                        }
                    ]
                })),
            )
            .await;
        assert_eq!(status, 422, "未进入导入阶段禁止应用: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn customer_row_import_requires_existing_party_via_d07() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_d07").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let party = entities::party::Party::new(
            entities::ids::PartyId::new("party-001"),
            entities::party::PartyData {
                party_no: "P-2026-001".to_string(),
                party_kind: entities::party::PartyKind::Enterprise,
                unified_credit_code: None,
                status: entities::party::PartyStatus::Active,
            },
            "test",
        )
        .unwrap();
        test_db
            .db()
            .parties()
            .create(&party, &mut database::NoTransaction)
            .await
            .unwrap();

        let (_, body) = api
            .post(
                "/admin/legacy-import-batches",
                Some(&token),
                Some(create_batch_json(
                    "IMP-CUST-1",
                    "CUSTOMER",
                    json!([row_json("CUSTOMER", "cust-1")]),
                )),
            )
            .await;
        assert_ok_envelope(200, &body);
        let batch_id = body["data"]["id"].as_str().unwrap().to_string();

        let (_, body) = api
            .post(
                "/admin/legacy-import-confirmations",
                Some(&token),
                Some(json!({
                    "batch_id": batch_id,
                    "confirmation_scope": "SALES",
                    "owner_role": "销售领导",
                    "batch_version": 1,
                    "trial_version": 1,
                    "import_rule_version": "v1",
                    "work_item_id": "wi-cust-1",
                })),
            )
            .await;
        let confirmation_id = body["data"]["id"].as_str().unwrap().to_string();
        let (_, body) = api
            .post(
                &format!("/admin/legacy-import-confirmations/{confirmation_id}/decide"),
                Some(&token),
                Some(json!({ "decision": "CONFIRM_SCOPE" })),
            )
            .await;
        assert_eq!(body["data"]["status"], "CONFIRMED");

        let (_, body) = api
            .get(
                &format!("/admin/legacy-import-batches/{batch_id}/rows"),
                Some(&token),
            )
            .await;
        let row_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-batches/{batch_id}/apply"),
                Some(&token),
                Some(json!({
                    "results": [
                        {
                            "row_id": row_id,
                            "outcome": "imported",
                            "external_identity_map_id": "eim-cust-1",
                            "target_document_id": "party-NOT-EXISTS"
                        }
                    ]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "partial_failed", "客户主体缺失按失败处理");
        assert_eq!(body["data"]["failed_rows"], 1);
        assert_eq!(body["data"]["success_rows"], 0);

        let (_, body) = api
            .get(
                &format!("/admin/legacy-import-batches/{batch_id}/rows"),
                Some(&token),
            )
            .await;
        assert_eq!(body["data"]["items"][0]["import_status"], "failed");
        assert_eq!(body["data"]["items"][0]["error_code"], "CUSTOMER_NOT_FOUND");

        let (_, body) = api
            .get(&format!("/admin/legacy-import-batches/{batch_id}"), Some(&token))
            .await;
        assert_eq!(
            body["data"]["status"], "partial_failed",
            "批次终态后重复应用按幂等返回"
        );

        let (status, body) = api
            .post(
                &format!("/admin/legacy-import-batches/{batch_id}/apply"),
                Some(&token),
                Some(json!({
                    "results": [
                        {
                            "row_id": row_id,
                            "outcome": "imported",
                            "external_identity_map_id": "eim-cust-1",
                            "target_document_id": "party-001"
                        }
                    ]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["id"].as_str().unwrap(),
            batch_id,
            "终态批次应用为幂等无操作"
        );
        let (_, body) = api
            .get(&format!("/admin/legacy-import-batches/{batch_id}"), Some(&token))
            .await;
        assert_eq!(body["data"]["status"], "partial_failed");
        assert_eq!(body["data"]["failed_rows"], 1, "不产生重复事实");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_boundaries() {
    require_mongo!(async {
        let test_db = TestDb::new("legacy_import_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        for index in 1..=25 {
            let (_, body) = api
                .post(
                    "/admin/legacy-import-batches",
                    Some(&token),
                    Some(create_batch_json(
                        &format!("IMP-PAGE-{index:03}"),
                        "CARD_SALES_ORDER",
                        json!([row_json("CARD_SALES_ORDER", "row-1")]),
                    )),
                )
                .await;
            assert_ok_envelope(200, &body);
        }

        let (status, body) = api
            .get("/admin/legacy-import-batches?page=2&page_size=10", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 25);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 10);
        assert_eq!(body["data"]["page"], 2);

        let (status, body) = api
            .get("/admin/legacy-import-batches?page=3&page_size=10", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["items"].as_array().unwrap().len(),
            5,
            "边界页返回剩余条数"
        );

        let (status, body) = api
            .get(
                "/admin/legacy-import-batches?sort_by=id&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "排序字段不在白名单必须 400: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .get("/admin/legacy-import-batches?page_size=101", Some(&token))
            .await;
        assert_eq!(status, 400, "分页大小超限必须 400: {body}");
    })
}
