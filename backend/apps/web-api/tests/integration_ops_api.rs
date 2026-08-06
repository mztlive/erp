//! 域 D34 `integration_ops` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test integration_ops_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入
//! `inbox_message`/`integration_error_task`/`reconciliation_difference`
//! 的直接 `p` 规则（casbin 的 `g(r.sub, p.sub)` 自反匹配，无需改角色），
//! 使 happy path 可鉴权通过，同时天然构造 403 用例。
//!
//! 覆盖：401/403/400/422、happy path 契约形状、409（乐观锁/唯一键）、
//! 事务不变量（任务终结+审计同时生效；注入失败全部不可见）、
//! REPLAY 原键锁定与幂等、分页与排序边界。

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
    ("inbox_message", "list"),
    ("inbox_message", "detail"),
    ("inbox_message", "register"),
    ("inbox_message", "writeback"),
    ("integration_error_task", "list"),
    ("integration_error_task", "detail"),
    ("integration_error_task", "create"),
    ("integration_error_task", "query"),
    ("integration_error_task", "replay"),
    ("integration_error_task", "hold"),
    ("integration_error_task", "transfer"),
    ("integration_error_task", "resolve"),
    ("integration_error_task", "close"),
    ("reconciliation_difference", "list"),
    ("reconciliation_difference", "detail"),
    ("reconciliation_difference", "create"),
    ("reconciliation_difference", "process"),
    ("reconciliation_difference", "resolve"),
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

/// 直接写入一个来源系统（D01 Repository，跨域只读依赖；测试数据直连仓储）。
async fn seed_source_system(db: &Database, code: &str) {
    use database::{NoTransaction, SourceRegistryExt};
    use entities::source_registry::{
        SourceSystem, SourceSystemData, SourceSystemId, SourceSystemStatus, SourceSystemType,
    };

    let system = SourceSystem::new(
        SourceSystemId::new(code.to_string()),
        SourceSystemData {
            code: code.to_string(),
            system_type: SourceSystemType::Mall,
            name: "测试商城".to_string(),
            status: SourceSystemStatus::Active,
        },
        "seed-actor",
    )
    .expect("来源系统实体构造失败");
    db.source_systems()
        .create(&system, &mut NoTransaction)
        .await
        .expect("写入来源系统失败");
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

/// 统计指定动作的审计日志条数。
async fn count_audit_logs(db: &Database, action: &str) -> u64 {
    db.collection::<Document>("audit_logs")
        .count_documents(doc! { "action": action })
        .await
        .expect("查询审计日志失败")
}

/// 测试基建：独立随机库名 + 索引 + 种子账号 + 本域权限 + JWT + 路由。
async fn setup(prefix: &str, with_permissions: bool) -> (TestDb, String, Router) {
    let test_db = TestDb::new(prefix).await.unwrap();
    database::ensure_indexes(test_db.db()).await.unwrap();
    seed_source_system(test_db.db(), "MALL").await;
    let account_id = seed_admin_account(test_db.db()).await.unwrap();
    if with_permissions {
        grant_domain_permissions(test_db.db(), &account_id).await;
    }
    let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
    let (router, upload_path) = build_router(&test_db).await;
    let _ = upload_path;
    (test_db, token, router)
}

/// 登记一条入站消息并返回消息 ID。
async fn register_message(api: &TestApi, token: &str, source_event_id: &str) -> String {
    let (status, body) = api
        .post(
            "/admin/integration/inbox-messages",
            Some(token),
            Some(json!({
                "source_system_id": "MALL",
                "source_event_id": source_event_id,
                "message_type": "PAYMENT_SUCCEEDED",
                "business_fact_key": format!("MALL|PAYMENT_SUCCEEDED|{source_event_id}|v1"),
                "payload_schema_version": "v1.2",
                "payload_reference": format!("archive://2026/{source_event_id}"),
                "source_sent_at": 1754438400,
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore]
async fn happy_path_register_message_list_and_write_back_processed() {
    require_mongo!(async {
        let (_test_db, token, router) = setup("int_ops_api_happy", true).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/integration/inbox-messages", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["items"], json!([]), "初始列表为空");
        assert_eq!(data["total"], 0);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);

        let (status, body) = api
            .post(
                "/admin/integration/inbox-messages",
                Some(&token),
                Some(json!({
                    "source_system_id": "MALL",
                    "source_event_id": "evt-1001",
                    "message_type": "PAYMENT_SUCCEEDED",
                    "business_fact_key": "MALL|PAYMENT_SUCCEEDED|SO-2026-001|v1",
                    "payload_schema_version": "v1.2",
                    "payload_reference": "archive://2026/evt-1001",
                    "source_sent_at": 1754438400,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        for field in [
            "id",
            "source_system_id",
            "source_event_id",
            "message_type",
            "business_fact_key",
            "payload_schema_version",
            "payload_reference",
            "status",
            "received_at",
            "version",
            "created_at",
        ] {
            assert!(
                created.get(field).is_some(),
                "契约字段 {field} 必须存在: {created}"
            );
        }
        assert_eq!(created["source_system_id"], "MALL");
        assert_eq!(created["message_type"], "PAYMENT_SUCCEEDED");
        assert_eq!(created["status"], "received");
        assert_eq!(created["version"], 1);
        assert!(created["received_at"].as_i64().unwrap() > 0);
        let id = created["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/integration/inbox-messages",
                Some(&token),
                Some(json!({
                    "source_system_id": "MALL",
                    "source_event_id": "evt-1001",
                    "message_type": "PAYMENT_SUCCEEDED",
                    "business_fact_key": "MALL|PAYMENT_SUCCEEDED|SO-2026-001|v1",
                    "payload_schema_version": "v1.2",
                })),
            )
            .await;
        assert_eq!(status, 409, "同来源事件消息重复投递必须 409: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .post(
                "/admin/integration/inbox-messages",
                Some(&token),
                Some(json!({
                    "source_system_id": "MALL",
                    "source_event_id": "evt-1002",
                    "message_type": "PAYMENT_SUCCEEDED",
                    "business_fact_key": "MALL|PAYMENT_SUCCEEDED|SO-2026-001|v1",
                    "payload_schema_version": "v1.2",
                })),
            )
            .await;
        assert_eq!(
            status, 409,
            "同业务事实键重复投递必须 409（先消息去重再事实去重）: {body}"
        );

        let (status, body) = api
            .get("/admin/integration/inbox-messages?status=received", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["source_event_id"], "evt-1001");

        let (status, body) = api
            .post(
                &format!("/admin/integration/inbox-messages/{id}/result"),
                Some(&token),
                Some(json!({ "version": 1, "outcome": "processed" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "processed");
        assert!(body["data"]["processed_at"].as_i64().unwrap() > 0);
        assert_eq!(body["data"]["version"], 2, "回写成功版本递增");

        let (status, body) = api
            .get(&format!("/admin/integration/inbox-messages/{id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "processed");
        assert_eq!(body["data"]["payload_reference"], "archive://2026/evt-1001");
    })
}

#[tokio::test]
#[ignore]
async fn write_back_failed_creates_error_task_and_duplicate_injection_rolls_back() {
    require_mongo!(async {
        let (_test_db, token, router) = setup("int_ops_api_fail", true).await;
        let api = TestApi::new(router);

        let id = register_message(&api, &token, "evt-2001").await;

        let (status, body) = api
            .post(
                &format!("/admin/integration/inbox-messages/{id}/result"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "outcome": "failed",
                    "error_class": "transient_failure",
                    "owner_role": "ops",
                    "attempt_summary": "上游超时（脱敏）",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "failed", "失败回写置消息为失败");

        let (status, body) = api.get("/admin/integration/error-tasks", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1, "失败回写同事务登记错误任务");
        let task = &body["data"]["items"][0];
        assert_eq!(task["message_id"], id);
        assert_eq!(task["error_class"], "transient_failure");
        assert_eq!(task["status"], "pending");
        assert_eq!(task["owner_role"], "ops");

        // 事务不变量注入：预先存在同 (message, error_class) 进行中任务 → 事务内
        // 任务插入命中部分唯一索引 → 整个事务回滚，消息不得被置为失败。
        let second_id = register_message(&api, &token, "evt-2002").await;
        let (status, body) = api
            .post(
                "/admin/integration/error-tasks",
                Some(&token),
                Some(json!({
                    "message_id": second_id,
                    "error_class": "transient_failure",
                    "owner_role": "ops",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);

        let (status, body) = api
            .post(
                &format!("/admin/integration/inbox-messages/{second_id}/result"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "outcome": "failed",
                    "error_class": "transient_failure",
                })),
            )
            .await;
        assert_eq!(status, 409, "进行中任务重复登记必须 409: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .get(
                &format!("/admin/integration/inbox-messages/{second_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["status"], "received",
            "注入失败后事务回滚：消息状态不得变成 failed"
        );
        let (status, body) = api.get("/admin/integration/error-tasks", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2, "注入失败后不得遗留第二份任务");
    })
}

#[tokio::test]
#[ignore]
async fn error_task_query_replay_transfer_resolve_flow() {
    require_mongo!(async {
        let (test_db, token, router) = setup("int_ops_api_flow", true).await;
        let api = TestApi::new(router);

        let message_id = register_message(&api, &token, "evt-3001").await;
        let (status, body) = api
            .post(
                &format!("/admin/integration/inbox-messages/{message_id}/result"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "outcome": "failed",
                    "error_class": "result_unknown",
                    "owner_role": "ops",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (_, body) = api.get("/admin/integration/error-tasks", Some(&token)).await;
        let task_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();

        // REPLAY 前置：结果未知必须先查询且明确无结果。
        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/replay"),
                Some(&token),
                Some(json!({ "version": 1 })),
            )
            .await;
        assert_eq!(status, 400, "结果未知未查询直接重放必须 400: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/query"),
                Some(&token),
                Some(json!({ "version": 1, "outcome": "result_unknown" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "pending", "查询后任务保持非终结");
        assert_eq!(body["data"]["attempt_count"], 1);
        let version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/replay"),
                Some(&token),
                Some(json!({ "version": version })),
            )
            .await;
        assert_eq!(status, 400, "仍未知时不得重放: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/query"),
                Some(&token),
                Some(json!({ "version": version, "outcome": "no_result_confirmed", "comment": "上游已确认无结果" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/replay"),
                Some(&token),
                Some(json!({ "version": version, "comment": "系统沿用锁定原键" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let replayed = &body["data"];
        assert_eq!(replayed["replay_accepted"], true);
        assert_eq!(replayed["original_action_idempotency_key_locked"], true);
        let key_summary = replayed["original_action_idempotency_key_summary"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            !key_summary.contains("MALL|PAYMENT_SUCCEEDED"),
            "原键必须脱敏，不得暴露完整业务事实键"
        );
        assert_eq!(replayed["task_status"], "pending", "重放后任务仍非终结");

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/replay"),
                Some(&token),
                Some(json!({ "version": replayed["task_version"].as_u64().unwrap() })),
            )
            .await;
        assert_eq!(status, 409, "重复 REPLAY 请求必须被拒（原键锁定）: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/transfer"),
                Some(&token),
                Some(json!({ "version": replayed["task_version"].as_u64().unwrap(), "owner_role": "finance", "owner_user_id": "u-9" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["owner_role"], "finance");
        assert_eq!(body["data"]["owner_user_id"], "u-9");
        assert_eq!(body["data"]["status"], "pending", "转交不改任务状态");

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": body["data"]["version"].as_u64().unwrap(),
                    "resolution_type": "query_confirm",
                    "resolution": "查询确认原请求已成功，形成可验证终态",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "resolved", "解决后进入终态");
        assert_eq!(body["data"]["resolution_type"], "query_confirm");

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/query"),
                Some(&token),
                Some(json!({ "version": body["data"]["version"].as_u64().unwrap(), "outcome": "result_unknown" })),
            )
            .await;
        assert_eq!(status, 409, "终态任务禁止再操作: {body}");

        assert_eq!(
            count_audit_logs(test_db.db(), "integration_error_task.resolve").await,
            1,
            "解决动作必须写审计日志（任务+审计同事务）"
        );
    })
}

#[tokio::test]
#[ignore]
async fn replay_rejects_client_supplied_idempotency_key() {
    require_mongo!(async {
        let (test_db, token, router) = setup("int_ops_api_replay", true).await;
        let api = TestApi::new(router);

        let message_id = register_message(&api, &token, "evt-4001").await;
        let (status, body) = api
            .post(
                &format!("/admin/integration/inbox-messages/{message_id}/result"),
                Some(&token),
                Some(json!({ "version": 1, "outcome": "failed", "error_class": "result_unknown" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (_, body) = api.get("/admin/integration/error-tasks", Some(&token)).await;
        let task_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();

        let (_, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/query"),
                Some(&token),
                Some(json!({ "version": 1, "outcome": "no_result_confirmed" })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/replay"),
                Some(&token),
                Some(json!({
                    "version": version,
                    "originalActionIdempotencyKey": "MALL|PAYMENT_SUCCEEDED|SO-999|v9",
                })),
            )
            .await;
        assert_eq!(
            status, 422,
            "REPLAY 请求携带客户端原键必须被拒（DTO deny_unknown_fields）: {body}"
        );
        assert_eq!(
            count_audit_logs(test_db.db(), "integration_error_task.replay").await,
            0
        );
    })
}

#[tokio::test]
#[ignore]
async fn error_task_close_requires_replacement_and_rejects_result_unknown() {
    require_mongo!(async {
        let (test_db, token, router) = setup("int_ops_api_close", true).await;
        let api = TestApi::new(router);

        let message_id = register_message(&api, &token, "evt-5001").await;
        let (status, body) = api
            .post(
                &format!("/admin/integration/inbox-messages/{message_id}/result"),
                Some(&token),
                Some(json!({ "version": 1, "outcome": "failed", "error_class": "transient_failure" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (_, body) = api.get("/admin/integration/error-tasks", Some(&token)).await;
        let task_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/close"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "reason": "duplicate",
                    "resolution": "重复任务",
                })),
            )
            .await;
        assert_eq!(status, 400, "重复关闭必须提供替代任务: {body}");

        let (status, body) = api
            .post(
                "/admin/integration/error-tasks",
                Some(&token),
                Some(json!({
                    "business_object_id": "so-2026-999",
                    "error_class": "mapping_error",
                    "owner_role": "ops",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let replacement_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/close"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "reason": "duplicate",
                    "resolution": format!("重复任务，替代任务 {replacement_id}"),
                    "replacement_task_id": replacement_id,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "closed");
        assert_eq!(body["data"]["resolution_type"], "close");

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/close"),
                Some(&token),
                Some(json!({
                    "version": body["data"]["version"].as_u64().unwrap(),
                    "reason": "misrouted",
                    "resolution": "再次关闭",
                })),
            )
            .await;
        assert_eq!(status, 409, "终态任务禁止重复关闭: {body}");
        assert_eq!(
            count_audit_logs(test_db.db(), "integration_error_task.close").await,
            1,
            "失败关闭不得产生审计"
        );

        // 结果未知任务不得以通用关闭退出（实体校验 → 422）。
        let unknown_message = register_message(&api, &token, "evt-5002").await;
        let (status, body) = api
            .post(
                &format!("/admin/integration/inbox-messages/{unknown_message}/result"),
                Some(&token),
                Some(json!({ "version": 1, "outcome": "failed", "error_class": "result_unknown" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (_, body) = api
            .get(
                "/admin/integration/error-tasks?error_class=result_unknown",
                Some(&token),
            )
            .await;
        let unknown_task_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{unknown_task_id}/close"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "reason": "misrouted",
                    "resolution": "证据说明",
                })),
            )
            .await;
        assert_eq!(status, 422, "结果未知任务禁止通用关闭: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{unknown_task_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "resolution_type": "close",
                    "resolution": "用关闭方式解决",
                })),
            )
            .await;
        assert_eq!(status, 422, "解决不得使用“关闭”方式: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn hold_keeps_task_in_open_queue() {
    require_mongo!(async {
        let (test_db, token, router) = setup("int_ops_api_hold", true).await;
        let api = TestApi::new(router);

        let message_id = register_message(&api, &token, "evt-6001").await;
        let (status, body) = api
            .post(
                &format!("/admin/integration/inbox-messages/{message_id}/result"),
                Some(&token),
                Some(json!({ "version": 1, "outcome": "failed", "error_class": "transient_failure" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (_, body) = api.get("/admin/integration/error-tasks", Some(&token)).await;
        let task_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/hold"),
                Some(&token),
                Some(json!({ "version": 1, "kind": "defer", "reason_code": "WAIT_SUPPLIER", "comment": "等供应商回复" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "pending", "暂挂保留在队列，状态不变");
        let version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .get("/admin/integration/error-tasks?status=pending", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1, "暂挂后任务仍在开放队列");

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/hold"),
                Some(&token),
                Some(json!({ "version": version, "kind": "skip" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "pending");
        assert_eq!(body["data"]["attempt_count"], 2, "跳过也追加尝试摘要");
        assert!(count_audit_logs(test_db.db(), "integration_error_task.defer").await >= 1);
        assert!(count_audit_logs(test_db.db(), "integration_error_task.skip").await >= 1);
    })
}

#[tokio::test]
#[ignore]
async fn reconciliation_difference_process_and_resolve_with_history() {
    require_mongo!(async {
        let (test_db, token, router) = setup("int_ops_api_diff", true).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/integration/differences",
                Some(&token),
                Some(json!({
                    "business_object_type": "mall_order",
                    "business_object_id": "MO-2026-001",
                    "difference_type": "amount_mismatch",
                    "left_fact_reference": "mall_order_fact://f-1001",
                    "right_fact_reference": "invoice://inv-88",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        assert_eq!(created["business_object_id"], "MO-2026-001");
        assert_eq!(created["difference_type"], "amount_mismatch");
        assert_eq!(created["status"], Value::Null, "无处理记录时状态为 null");
        let difference_id = created["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .get(
                "/admin/integration/differences?business_object_type=mall_order",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["status"], Value::Null);

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/process"),
                Some(&token),
                Some(json!({ "version": 0, "action": "claim" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["resolution_no"], 1);
        assert_eq!(body["data"]["resolution_action"], "claim");
        assert_eq!(body["data"]["resulting_status"], "in_progress");
        assert!(
            !body["data"]["handled_by"].as_str().unwrap().is_empty(),
            "处理人必须是当前操作人"
        );
        assert_eq!(body["data"]["version"], 1, "响应回传最新处理序号作为乐观锁令牌");
        let version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/process"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "action": "claim",
                    "comment": "重复领取",
                })),
            )
            .await;
        assert_eq!(status, 400, "领取仅允许作为首条处理记录: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/process"),
                Some(&token),
                Some(json!({
                    "version": version,
                    "action": "add_evidence",
                    "evidence_reference": "supplier_order_action://act-7",
                    "comment": "补充供应商动作证据",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["resolution_no"], 2);
        assert_eq!(body["data"]["resolution_action"], "processing");
        assert_eq!(body["data"]["version"], 2);
        let version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .get(
                &format!("/admin/integration/differences/{difference_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "in_progress");
        assert_eq!(body["data"]["resolutions"].as_array().unwrap().len(), 2);

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": version,
                    "conclusion": "confirm_valid_difference",
                    "reason_code": "SOURCE_CORRECTED_AND_REATTRIBUTED",
                    "evidence_reference": "sales_change_order://co-7",
                    "comment": "来源已更正并重新归集",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["resolution_no"], 3);
        assert_eq!(body["data"]["resolution_action"], "resolved");
        assert_eq!(body["data"]["resulting_status"], "resolved");
        assert_eq!(body["data"]["version"], 3);
        let evidence = body["data"]["evidence_reference"].as_str().unwrap();
        assert!(
            evidence.contains("reason_code=SOURCE_CORRECTED_AND_REATTRIBUTED"),
            "固定原因枚举必须写入处理记录证据引用: {evidence}"
        );

        let (status, body) = api
            .get(
                "/admin/integration/differences?business_object_type=mall_order",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"][0]["status"], "resolved", "列表派生终态");

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/process"),
                Some(&token),
                Some(json!({ "version": 1, "action": "processing" })),
            )
            .await;
        assert_eq!(status, 409, "已终结差异禁止继续处理（陈旧序号）: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": 3,
                    "conclusion": "confirm_no_error",
                    "reason_code": "BUSINESS_CONFIRMED_NO_ERROR",
                    "evidence_reference": "audit://x",
                })),
            )
            .await;
        assert_eq!(status, 409, "已终结差异禁止再次解决: {body}");

        assert_eq!(
            count_audit_logs(test_db.db(), "reconciliation_difference.resolve").await,
            1
        );
        assert_eq!(
            count_audit_logs(test_db.db(), "reconciliation_difference.process").await,
            2
        );
    })
}

#[tokio::test]
#[ignore]
async fn reconciliation_difference_rejects_duplicate_and_invalid_inputs() {
    require_mongo!(async {
        let (_test_db, token, router) = setup("int_ops_api_diff_bad", true).await;
        let api = TestApi::new(router);

        let payload = json!({
            "business_object_type": "supplier_order",
            "business_object_id": "SFO-2026-001",
            "difference_type": "status_mismatch",
            "left_fact_reference": "supplier_fulfillment_order://sfo-1",
        });
        let (status, body) = api
            .post(
                "/admin/integration/differences",
                Some(&token),
                Some(payload.clone()),
            )
            .await;
        assert_ok_envelope(status, &body);
        let difference_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post("/admin/integration/differences", Some(&token), Some(payload))
            .await;
        assert_eq!(status, 409, "同对象同分类差异重复登记必须 409: {body}");

        let (status, body) = api
            .post(
                "/admin/integration/differences",
                Some(&token),
                Some(json!({
                    "business_object_type": "supplier_order",
                    "business_object_id": "SFO-2",
                    "difference_type": "status_mismatch",
                })),
            )
            .await;
        assert_eq!(status, 400, "两侧证据都缺失必须 400: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "conclusion": "confirm_no_error",
                    "reason_code": "FREE_TEXT_REASON",
                    "evidence_reference": "audit://x",
                })),
            )
            .await;
        assert_eq!(status, 422, "自由文本原因必须被拒（固定枚举）: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": 0,
                    "conclusion": "confirm_no_error",
                    "reason_code": "BUSINESS_CONFIRMED_NO_ERROR",
                    "evidence_reference": "  ",
                })),
            )
            .await;
        assert_eq!(status, 400, "受控证据不能为空: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let (test_db, _, router) = setup("int_ops_api_401", false).await;
        let _ = test_db;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/integration/inbox-messages", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let (test_db, token, router) = setup("int_ops_api_403", false).await;
        let _ = test_db;
        // 种子账号只有 role/admin/audit_log.list 权限，本域权限未授予 → 403。
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/integration/error-tasks", Some(&token)).await;
        assert_eq!(status, 403, "无 integration_error_task.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let (test_db, token, router) = setup("int_ops_api_400", true).await;
        let _ = test_db;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/integration/inbox-messages",
                Some(&token),
                Some(json!({
                    "source_system_id": "MALL",
                    "source_event_id": "  ",
                    "message_type": "PAYMENT_SUCCEEDED",
                    "business_fact_key": "k-1",
                    "payload_schema_version": "v1",
                })),
            )
            .await;
        assert_eq!(status, 400, "空白来源事件 ID 必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null, "失败时 data 恒为 null");

        let (status, _) = api
            .post(
                "/admin/integration/inbox-messages",
                Some(&token),
                Some(json!({ "source_system_id": "MALL" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/integration/inbox-messages",
                Some(&token),
                Some(json!({
                    "source_system_id": "MALL",
                    "source_event_id": "e-1",
                    "message_type": "PAYMENT_DECLINED",
                    "business_fact_key": "k-1",
                    "payload_schema_version": "v1",
                })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");

        let (status, body) = api
            .post(
                "/admin/integration/differences",
                Some(&token),
                Some(json!({
                    "business_object_type": "mall_order",
                    "business_object_id": "MO-1",
                    "difference_type": "amount_mismatch",
                    "left_fact_reference": "x",
                    "right_fact_reference": "y",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let difference_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": 99999999999_i64,
                    "conclusion": "confirm_no_error",
                    "reason_code": "BUSINESS_CONFIRMED_NO_ERROR",
                    "evidence_reference": "audit://x",
                })),
            )
            .await;
        assert_eq!(status, 400, "处理序号越界必须 400: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_writes_return_409_and_leave_no_partial_state() {
    require_mongo!(async {
        let (test_db, token, router) = setup("int_ops_api_409", true).await;
        let api = TestApi::new(router);

        let message_id = register_message(&api, &token, "evt-7001").await;
        let (status, body) = api
            .post(
                &format!("/admin/integration/inbox-messages/{message_id}/result"),
                Some(&token),
                Some(json!({ "version": 1, "outcome": "failed", "error_class": "result_unknown" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (_, body) = api.get("/admin/integration/error-tasks", Some(&token)).await;
        let task_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "resolution_type": "query_confirm",
                    "resolution": "查询确认已成功",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/integration/error-tasks/{task_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "resolution_type": "query_confirm",
                    "resolution": "陈旧版本解决",
                })),
            )
            .await;
        assert_eq!(status, 409, "陈旧版本写入必须 409: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .get(&format!("/admin/integration/error-tasks/{task_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["version"], version, "失败写入不得改变版本");
        assert_eq!(
            count_audit_logs(test_db.db(), "integration_error_task.resolve").await,
            1
        );

        // 差异侧同样验证：陈旧版本解决不产生处理记录。
        let (status, body) = api
            .post(
                "/admin/integration/differences",
                Some(&token),
                Some(json!({
                    "business_object_type": "mall_order",
                    "business_object_id": "MO-409",
                    "difference_type": "status_mismatch",
                    "left_fact_reference": "mall_order_fact://f-409",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let difference_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/process"),
                Some(&token),
                Some(json!({ "version": 0, "action": "claim" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/integration/differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": 0,
                    "conclusion": "confirm_no_error",
                    "reason_code": "BUSINESS_CONFIRMED_NO_ERROR",
                    "evidence_reference": "audit://x",
                })),
            )
            .await;
        assert_eq!(status, 409, "差异陈旧序号解决必须 409: {body}");

        let (status, body) = api
            .get(
                &format!("/admin/integration/differences/{difference_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["resolutions"].as_array().unwrap().len(),
            1,
            "失败解决不得遗留处理记录"
        );
        assert_eq!(body["data"]["version"], version, "失败写入不得改变差异状态");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_boundaries() {
    require_mongo!(async {
        let (_test_db, token, router) = setup("int_ops_api_page", true).await;
        let api = TestApi::new(router);

        for index in 1..=3 {
            let _ = register_message(&api, &token, &format!("evt-page-{index}")).await;
        }

        let (status, body) = api
            .get(
                "/admin/integration/inbox-messages?page=1&page_size=2&sort_by=received_at&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 3);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 2);
        assert_eq!(data["items"].as_array().unwrap().len(), 2);

        let (status, body) = api
            .get(
                "/admin/integration/inbox-messages?page=2&page_size=2",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["items"].as_array().unwrap().len(),
            1,
            "边界页只剩 1 条"
        );

        let (status, body) = api
            .get(
                "/admin/integration/inbox-messages?sort_by=payload_schema_version",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");

        let (status, body) = api
            .get("/admin/integration/inbox-messages?sort_dir=up", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序方向必须 400: {body}");

        let (status, body) = api
            .get("/admin/integration/inbox-messages?page=0", Some(&token))
            .await;
        assert_eq!(status, 400, "页码必须大于 0: {body}");

        let (status, body) = api
            .get("/admin/integration/inbox-messages?page_size=200", Some(&token))
            .await;
        assert_eq!(status, 400, "分页大小必须在 1-100: {body}");
    })
}
