//! 域 D08 `customer` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）。
//! 鉴权链路与生产一致：种子账号只有 `role/admin/audit_log.list` 权限，
//! 本测试额外为种子账号插入本域直接 `p` 规则，天然构造 403 用例。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use database::{NoTransaction, PartyExt};
use entities::party::{Party, PartyData, PartyId, PartyKind, PartyStatus};
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
    ("customer", "list"),
    ("customer", "create"),
    ("customer", "detail"),
    ("customer", "update"),
    ("customer", "delete"),
    ("customer_assignment", "list"),
    ("customer_assignment", "create"),
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

/// 直接经仓储写入一个主体（测试前置数据，避免依赖 party API）。
async fn seed_party(db: &Database, party_no: &str) -> String {
    let id = PartyId::new(format!("party-{party_no}"));
    let party = Party::new(
        id.clone(),
        PartyData {
            party_no: party_no.to_string(),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: None,
            status: PartyStatus::Active,
        },
        "seed",
    )
    .unwrap();
    db.parties().create(&party, &mut NoTransaction).await.unwrap();
    id.to_string()
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

/// 创建客户的请求体（负责销售使用种子账号）。
fn create_customer_body(party_id: &str, customer_no: &str, owner: &str) -> Value {
    json!({
        "party_id": party_id,
        "customer_no": customer_no,
        "owner_user_id": owner,
        "valid_from": "2026-01-01",
        "change_reason": "首次建档",
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("customer_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/customers", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("customer_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/customers", Some(&token)).await;
        assert_eq!(status, 403, "无 customer.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("customer_api_400").await.unwrap();
        let (_, api, token, account_id) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-C400").await;

        let (status, body) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(json!({
                    "party_id": party_id,
                    "customer_no": "  ",
                    "owner_user_id": account_id,
                    "valid_from": "2026-01-01",
                    "change_reason": "x",
                })),
            )
            .await;
        assert_eq!(status, 400, "空白 customer_no 必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, _) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(json!({ "customer_no": "C-1" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(json!({
                    "party_id": party_id,
                    "customer_no": "C-1",
                    "owner_user_id": account_id,
                    "valid_from": "2026-01-01",
                    "change_reason": "x",
                    "status": "MARS",
                })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_create_customer_with_owner_assignment_and_detail() {
    require_mongo!(async {
        let test_db = TestDb::new("customer_api_happy").await.unwrap();
        let (_, api, token, account_id) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-CH1").await;

        let (status, body) = api.get("/admin/customers", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"], json!([]));

        let (status, body) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(create_customer_body(&party_id, "C-2026-001", &account_id)),
            )
            .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        assert_eq!(created["customer_no"], "C-2026-001");
        assert_eq!(created["party_id"], party_id);
        assert_eq!(created["status"], "active");
        assert_eq!(created["version"], 1);
        let customer_id = created["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .get(&format!("/admin/customers/{customer_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["customer_no"], "C-2026-001");
        assert_eq!(detail["party_no"], "P-CH1");
        assert_eq!(detail["owner_user_id"], account_id);

        let (status, body) = api
            .get(
                &format!("/admin/customers/{customer_id}/assignments"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1, "创建客户同事务建立首条 OWNER 归属");
        assert_eq!(data["items"][0]["assignment_role"], "OWNER");
        assert_eq!(data["items"][0]["user_id"], account_id);
        assert_eq!(data["items"][0]["valid_from"], "2026-01-01");
    })
}

#[tokio::test]
#[ignore]
async fn missing_party_or_owner_returns_404_and_duplicate_party_returns_409() {
    require_mongo!(async {
        let test_db = TestDb::new("customer_api_404").await.unwrap();
        let (_, api, token, account_id) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-C404").await;

        let (status, body) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(create_customer_body(
                    "party-does-not-exist",
                    "C-2026-099",
                    &account_id,
                )),
            )
            .await;
        assert_eq!(status, 404, "主体不存在必须 404: {body}");

        let (status, body) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(create_customer_body(
                    &party_id,
                    "C-2026-099",
                    "user-does-not-exist",
                )),
            )
            .await;
        assert_eq!(status, 404, "负责销售账号不存在必须 404: {body}");

        let (_, body) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(create_customer_body(&party_id, "C-2026-001", &account_id)),
            )
            .await;
        assert_ok_envelope(200, &body);

        let (status, body) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(create_customer_body(&party_id, "C-2026-002", &account_id)),
            )
            .await;
        assert_eq!(status, 409, "一个 party 最多一个有效客户角色必须 409: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn change_owner_ends_old_and_creates_new_owner_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("customer_api_owner").await.unwrap();
        let (_, api, token, account_id) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-CO1").await;
        // 第二个销售账号（换负责人目标）。
        let other_user = seed_admin_account(test_db.db()).await.unwrap();

        let (_, body) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(create_customer_body(&party_id, "C-2026-010", &account_id)),
            )
            .await;
        assert_ok_envelope(200, &body);
        let customer_id = body["data"]["id"].as_str().unwrap().to_string();
        let old_assignment_id = api
            .get(
                &format!("/admin/customers/{customer_id}/assignments"),
                Some(&token),
            )
            .await
            .1["data"]["items"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        // 换负责人：新 OWNER 2026-06-01 起，旧 OWNER 被结束到 2026-05-31。
        let (status, body) = api
            .post(
                &format!("/admin/customers/{customer_id}/assignments"),
                Some(&token),
                Some(json!({
                    "action": "assign",
                    "user_id": other_user,
                    "assignment_role": "OWNER",
                    "valid_from": "2026-06-01",
                    "valid_to": "2027-05-31",
                    "change_reason": "换负责人",
                    "version": 1,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let changed = body["data"].as_array().unwrap();
        assert_eq!(changed.len(), 2, "返回被结束的旧归属 + 新归属");
        let old_ended = changed
            .iter()
            .find(|item| item["id"] == json!(old_assignment_id))
            .expect("旧归属必须出现在变更结果中");
        assert_eq!(
            old_ended["valid_to"], "2026-05-31",
            "旧 OWNER 被结束到新开始前一天"
        );
        let new_owner = changed
            .iter()
            .find(|item| item["user_id"] == json!(other_user))
            .expect("新 OWNER 必须出现");
        assert_eq!(new_owner["assignment_role"], "OWNER");

        let (status, body) = api
            .get(
                &format!("/admin/customers/{customer_id}/assignments"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let items = body["data"]["items"].as_array().unwrap();
        let new_owners: Vec<&Value> = items
            .iter()
            .filter(|item| {
                item["assignment_role"] == "OWNER"
                    && item["valid_from"] == "2026-06-01"
                    && item["valid_to"] == "2027-05-31"
            })
            .collect();
        assert_eq!(new_owners.len(), 1, "同一时点恰好一个生效 OWNER");
        let ended_owners: Vec<&Value> = items
            .iter()
            .filter(|item| item["assignment_role"] == "OWNER" && item["valid_to"].is_string())
            .collect();
        assert_eq!(ended_owners.len(), 2, "旧 OWNER 已结束，新旧各一条且无重叠");

        let (status, body) = api
            .get(&format!("/admin/customers/{customer_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["owner_user_id"], other_user, "当前负责人已切换");
    })
}

#[tokio::test]
#[ignore]
async fn end_assignment_with_stale_version_returns_409() {
    require_mongo!(async {
        let test_db = TestDb::new("customer_api_end").await.unwrap();
        let (_, api, token, account_id) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-CE1").await;

        let (_, body) = api
            .post(
                "/admin/customers",
                Some(&token),
                Some(create_customer_body(&party_id, "C-2026-020", &account_id)),
            )
            .await;
        assert_ok_envelope(200, &body);
        let customer_id = body["data"]["id"].as_str().unwrap().to_string();
        let assignment_id = api
            .get(
                &format!("/admin/customers/{customer_id}/assignments"),
                Some(&token),
            )
            .await
            .1["data"]["items"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let (status, body) = api
            .post(
                &format!("/admin/customers/{customer_id}/assignments"),
                Some(&token),
                Some(json!({
                    "action": "end",
                    "assignment_id": assignment_id,
                    "valid_to": "2026-03-31",
                    "change_reason": "提前结束",
                    "version": 1,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"][0]["valid_to"], "2026-03-31");

        let (status, body) = api
            .post(
                &format!("/admin/customers/{customer_id}/assignments"),
                Some(&token),
                Some(json!({
                    "action": "end",
                    "assignment_id": assignment_id,
                    "valid_to": "2026-04-30",
                    "change_reason": "陈旧提交",
                    "version": 1,
                })),
            )
            .await;
        assert_eq!(status, 409, "陈旧版本结束必须 409: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_are_validated() {
    require_mongo!(async {
        let test_db = TestDb::new("customer_api_page").await.unwrap();
        let (_, api, token, account_id) = setup(&test_db).await;

        for (i, party_no) in ["P-CP1", "P-CP2", "P-CP3"].iter().enumerate() {
            let party_id = seed_party(test_db.db(), party_no).await;
            let (status, body) = api
                .post(
                    "/admin/customers",
                    Some(&token),
                    Some(create_customer_body(
                        &party_id,
                        &format!("C-2026-03{i}"),
                        &account_id,
                    )),
                )
                .await;
            assert_ok_envelope(status, &body);
        }

        let (status, body) = api
            .get(
                "/admin/customers?page=2&page_size=2&sort_by=customer_no&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["items"][0]["customer_no"], "C-2026-032");

        let (status, body) = api.get("/admin/customers?sort_by=hacked", Some(&token)).await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");

        let (status, body) = api.get("/admin/customers?keyword=C-2026-03", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3, "编号模糊搜索命中全部");
    })
}
