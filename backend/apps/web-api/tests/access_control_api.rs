//! 域 D06 `access_control` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test access_control_api -- --include-ignored`。
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
    ("permission", "list"),
    ("permission", "create"),
    ("permission", "update"),
    ("permission", "delete"),
    ("data_scope", "list"),
    ("data_scope", "create"),
    ("data_scope", "delete"),
    ("user_role", "list"),
    ("user_role", "create"),
    ("user_role", "revoke"),
    ("audit_event", "list"),
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

/// 创建一个自定义权限定义并返回其 ID。
async fn create_permission(api: &TestApi, token: &str) -> String {
    let (status, body) = api
        .post(
            "/admin/permissions",
            Some(token),
            Some(json!({
                "resource": "sales_order",
                "action": "approve",
                "name": "销售单审批",
                "description": "审批销售单",
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore]
async fn happy_path_permission_crud_and_data_scope_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("access_ctl_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (status, body) = api.get("/admin/permissions", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0);

        let id = create_permission(&api, &token).await;
        let (status, body) = api
            .get("/admin/permissions?resource=sales_order", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        let row = &body["data"]["items"][0];
        assert_eq!(row["resource"], "sales_order");
        assert_eq!(row["action"], "approve");
        assert_eq!(row["system"], false);
        assert_eq!(row["disabled"], false);

        let (status, body) = put_json(
            &router,
            &format!("/admin/permissions/{id}"),
            &token,
            json!({ "version": 1, "name": "销售单审批（已更新）", "disabled": true }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["name"], "销售单审批（已更新）");
        assert_eq!(body["data"]["disabled"], true);
        assert_eq!(body["data"]["version"], 2);
        assert_eq!(body["data"]["resource"], "sales_order", "身份字段不可修改");

        let (status, body) = put_json(
            &router,
            &format!("/admin/permissions/{id}"),
            &token,
            json!({ "version": 1, "name": "陈旧版本" }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本更新必须 409: {body}");
        assert_eq!(body["success"], false);

        // 数据范围：团队范围必须携带目标；公司范围不允许携带目标。
        let (status, body) = api
            .post(
                "/admin/data-scopes",
                Some(&token),
                Some(json!({
                    "subject_type": "role",
                    "subject_id": "role-sales",
                    "scope_type": "team",
                    "scope_targets": ["team-1", "team-2"],
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let scope = &body["data"];
        assert_eq!(scope["subject_type"], "role");
        assert_eq!(scope["scope_type"], "team");
        assert_eq!(scope["scope_targets"], json!(["team-1", "team-2"]));
        let scope_id = scope["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .get(
                "/admin/data-scopes?subject_type=role&subject_id=role-sales",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);

        let (status, body) = delete_request(&router, &format!("/admin/data-scopes/{scope_id}"), &token).await;
        assert_ok_envelope(status, &body);
        let (status, body) = api.get("/admin/data-scopes", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "软删除后列表不可见");

        // 审计事件：每次写入都留痕（audit_log → audit_event 字段对齐）。
        let (status, body) = api.get("/admin/audit-events", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert!(body["data"]["total"].as_i64().unwrap() >= 4);
        let row = &body["data"]["items"][0];
        for field in [
            "id",
            "actor_id",
            "actor_label",
            "actor_role",
            "action_type",
            "object_type",
            "result",
        ] {
            assert!(row.get(field).is_some(), "审计契约字段 {field} 必须存在");
        }

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn permission_and_data_scope_unique_conflicts_return_409() {
    require_mongo!(async {
        let test_db = TestDb::new("access_ctl_api_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        create_permission(&api, &token).await;
        let (status, body) = api
            .post(
                "/admin/permissions",
                Some(&token),
                Some(json!({
                    "resource": "sales_order",
                    "action": "approve",
                    "name": "重复定义",
                })),
            )
            .await;
        assert_eq!(status, 409, "同 resource:action 重复定义必须 409: {body}");
        assert_eq!(body["success"], false);

        let payload = json!({
            "subject_type": "role",
            "subject_id": "role-sales",
            "scope_type": "team",
            "scope_targets": ["team-1"],
        });
        let (_, body) = api
            .post("/admin/data-scopes", Some(&token), Some(payload.clone()))
            .await;
        assert_ok_envelope(200, &body);
        let (status, body) = api.post("/admin/data-scopes", Some(&token), Some(payload)).await;
        assert_eq!(status, 409, "同主体同范围类型重复必须 409: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn user_role_assign_and_revoke_writes_audit_events_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("access_ctl_api_roles").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // 种子角色：p0-test-* 前缀，从 roles 集合读取第一个非系统角色。
        let role = test_db
            .db()
            .collection::<Document>("roles")
            .find_one(doc! { "deleted_at": 0 })
            .await
            .unwrap()
            .expect("种子角色必须存在");
        let role_id = role.get_str("id").unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/user-roles",
                Some(&token),
                Some(json!({
                    "user_id": "user-1",
                    "role_id": role_id,
                    "effective_from": 1767225600,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let binding = &body["data"];
        assert_eq!(binding["user_id"], "user-1");
        assert_eq!(binding["effective_from"], 1767225600);
        let binding_id = binding["id"].as_str().unwrap().to_string();

        let (status, body) = api.get("/admin/user-roles?user_id=user-1", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"][0]["role_id"], role_id);

        let (status, body) = api
            .post(
                &format!("/admin/user-roles/{binding_id}/revoke"),
                Some(&token),
                Some(json!({
                    "version": 1,
                    "revoke_reason_code": "EMERGENCY_REVOKE",
                    "revoke_reason_text": "紧急撤权",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert!(body["data"]["revoked_at"].as_u64().unwrap() > 0);
        assert_eq!(body["data"]["revoke_reason_code"], "EMERGENCY_REVOKE");

        // 已撤权绑定不可重复撤权（实体校验）→ 422。
        let (status, body) = api
            .post(
                &format!("/admin/user-roles/{binding_id}/revoke"),
                Some(&token),
                Some(json!({ "version": 2, "revoke_reason_code": "EMERGENCY_REVOKE" })),
            )
            .await;
        assert_eq!(status, 422, "已撤权绑定重复撤权必须 422: {body}");

        // 事务不变量：分配与撤权各留一条审计事件。
        let count = test_db
            .db()
            .collection::<Document>("audit_events")
            .count_documents(doc! { "object_type": "user_role" })
            .await
            .unwrap();
        assert_eq!(count, 2, "分配与撤权各留一条审计事件");

        // 不存在的角色 → 404。
        let (status, body) = api
            .post(
                "/admin/user-roles",
                Some(&token),
                Some(json!({
                    "user_id": "user-2",
                    "role_id": "role-missing",
                    "effective_from": 1767225600,
                })),
            )
            .await;
        assert_eq!(status, 404, "角色不存在必须 404: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn system_permission_cannot_be_deleted() {
    require_mongo!(async {
        let test_db = TestDb::new("access_ctl_api_system").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (status, body) = api
            .post(
                "/admin/permissions",
                Some(&token),
                Some(json!({
                    "resource": "card_voucher",
                    "action": "issue",
                    "name": "系统发卡",
                    "system": true,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["system"], true);
        let id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = delete_request(&router, &format!("/admin/permissions/{id}"), &token).await;
        assert_eq!(status, 422, "系统内建权限禁止删除: {body}");
        assert_eq!(body["success"], false);

        // 自定义权限可删除。
        let custom = create_permission(&api, &token).await;
        let (status, body) = delete_request(&router, &format!("/admin/permissions/{custom}"), &token).await;
        assert_ok_envelope(status, &body);
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("access_ctl_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/audit-events", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("access_ctl_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/data-scopes", Some(&token)).await;
        assert_eq!(status, 403, "无 data_scope.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("access_ctl_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/data-scopes",
                Some(&token),
                Some(json!({
                    "subject_type": "role",
                    "subject_id": "role-sales",
                    "scope_type": "company",
                    "scope_targets": ["team-1"],
                })),
            )
            .await;
        assert_eq!(status, 400, "公司范围不允许携带目标必须 400: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .get("/admin/data-scopes?subject_id=role-sales", Some(&token))
            .await;
        assert_eq!(status, 400, "按主体查询缺少主体类型必须 400: {body}");

        let (status, _) = api
            .post(
                "/admin/user-roles",
                Some(&token),
                Some(json!({ "user_id": "user-1", "role_id": "role-sales", "effective_from": 0 })),
            )
            .await;
        assert_eq!(status, 422, "非法取值走 axum Json 拒绝或 DTO 校验 400");

        let (status, _) = api
            .get("/admin/audit-events?sort_by=actor_id", Some(&token))
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400");
    })
}

#[tokio::test]
#[ignore]
async fn audit_event_filters_are_applied() {
    require_mongo!(async {
        let test_db = TestDb::new("access_ctl_api_audit").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        create_permission(&api, &token).await;
        let (status, body) = api
            .get(
                &format!("/admin/audit-events?actor_id={account_id}&action_type=permission.create"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["action_type"], "permission.create");

        let (status, body) = api
            .get(
                &format!("/admin/audit-events?actor_id={account_id}&result=DENIED"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "本域写入恒为 SUCCESS");
    })
}
