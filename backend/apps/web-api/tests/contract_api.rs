//! 域 D12 `contract` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test contract_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入
//! `contract` 的直接 `p` 规则（casbin 的 `g(r.sub, p.sub)` 自反匹配，无需改角色）。

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
use axum::Router;
use config::{Config, SafeConfig};
use database::{CustomerExt, NoTransaction};
use entities::customer::{CustomerAccount, CustomerAccountData, CustomerAccountId, CustomerAccountStatus};
use entities::ids::PartyId;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use tower::ServiceExt;
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节，同时满足 config 校验与 test-support 签发要求）。
const TEST_JWT_SECRET: &str = "c-g4-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("contract", "list"),
    ("contract", "create"),
    ("contract", "detail"),
    ("contract", "update"),
];

/// 发送 POST 请求（`TestApi` 只提供 GET/POST，POST 带 JSON 体由本辅助覆盖）。
async fn post_json(router: &Router, path: &str, token: &str, json: Value) -> (u16, Value) {
    let request = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        )
        .header("content-type", "application/json")
        .body(Body::from(json.to_string()))
        .expect("POST 请求构造失败");
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

/// 种子客户（合同创建依赖 D08 `customer_accounts` 存在性校验）。
async fn seed_customer(db: &Database) -> String {
    let customer = CustomerAccount::new(
        CustomerAccountId::new("cust-c-g4-1"),
        CustomerAccountData {
            party_id: PartyId::new("party-c-g4-1"),
            customer_no: "C0001".to_string(),
            default_payment_term_id: None,
            status: CustomerAccountStatus::Active,
        },
        "seed",
    )
    .unwrap();
    db.customer_accounts()
        .create(&customer, &mut NoTransaction)
        .await
        .unwrap();
    customer.base.id
}

/// 构造最小 AppState（默认配置 + 临时上传目录）并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "c-g4-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c-g4-uploads-{}", uuid::Uuid::new_v4()));
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

/// 构造合同创建请求体。
fn contract_payload(customer_id: &str, contract_no: &str) -> Value {
    json!({
        "contract_no": contract_no,
        "customer_id": customer_id,
        "settlement_party_id": "party-c-g4-1",
        "contract_pdf_file_id": "file-c-g4-1",
        "archive_source": "CONTRACT_CENTER",
        "customer_name": "东方企业",
        "settlement_party_name": "集团结算中心",
        "payment_term_code": "NET30",
        "payment_term_name": "月结 30 天",
        "invoice_type": "增值税专用发票",
        "tax_point": "6",
        "valid_from": "2026-01-01",
        "valid_to": "2026-12-31",
        "signed_at": "2025-12-20"
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_create_contract_then_list_and_detail_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_contract_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (status, body) = api.get("/admin/contracts", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["items"], json!([]), "初始列表为空");
        assert_eq!(data["total"], 0);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);

        let (status, body) = post_json(
            &router,
            "/admin/contracts",
            &token,
            contract_payload(&customer_id, "HT-2026-0088"),
        )
        .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        assert_eq!(created["contract_no"], "HT-2026-0088");
        assert_eq!(created["customer_id"], customer_id);
        assert_eq!(created["status"], "EFFECTIVE");
        assert_eq!(created["version"], 1);
        assert!(!created["id"].as_str().unwrap().is_empty());
        let contract_id = created["id"].as_str().unwrap().to_string();

        let (status, body) = api.get("/admin/contracts", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        let item = &data["items"][0];
        for field in [
            "id",
            "contract_no",
            "customer_id",
            "settlement_party_id",
            "status",
            "version",
        ] {
            assert!(item.get(field).is_some(), "契约字段 {field} 必须存在: {item}");
        }
        assert_eq!(item["contract_no"], "HT-2026-0088");

        let (status, body) = api
            .get(&format!("/admin/contracts/{contract_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["status"], "EFFECTIVE");
        assert_eq!(detail["revisions"].as_array().unwrap().len(), 1);
        let revision = &detail["revisions"][0];
        assert_eq!(revision["revision_no"], 1);
        assert_eq!(revision["contract_pdf_file_id"], "file-c-g4-1");
        assert_eq!(revision["valid_from"], "2026-01-01");
        assert_eq!(revision["customer_name"], "东方企业");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn archive_revision_and_terminate_contract_with_optimistic_locking() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_contract_lifecycle").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (_, body) = post_json(
            &router,
            "/admin/contracts",
            &token,
            contract_payload(&customer_id, "HT-2026-0099"),
        )
        .await;
        assert_ok_envelope(200, &body);
        let contract_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = post_json(
            &router,
            &format!("/admin/contracts/{contract_id}/revisions"),
            &token,
            json!({
                "version": 1,
                "contract_pdf_file_id": "file-c-g4-2",
                "archive_source": "SALES_ORDER_CREATE",
                "customer_name": "东方企业",
                "settlement_party_name": "集团结算中心",
                "payment_term_code": "NET30",
                "payment_term_name": "月结 30 天",
                "invoice_type": "增值税专用发票",
                "tax_point": "6",
                "valid_from": "2026-01-01",
                "valid_to": "2027-12-31",
                "signed_at": "2026-02-01"
            }),
        )
        .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["version"], 2, "追加版本后主表版本递增");
        assert_eq!(detail["revisions"].as_array().unwrap().len(), 2);
        assert_eq!(detail["revisions"][0]["revision_no"], 2, "新版本在前");
        assert_eq!(detail["current_revision_id"], detail["revisions"][0]["id"]);

        let (status, body) = post_json(
            &router,
            &format!("/admin/contracts/{contract_id}/revisions"),
            &token,
            json!({
                "version": 1,
                "contract_pdf_file_id": "file-c-g4-3",
                "customer_name": "东方企业",
                "settlement_party_name": "集团结算中心",
                "payment_term_code": "NET30",
                "payment_term_name": "月结 30 天",
                "invoice_type": "增值税专用发票",
                "tax_point": "6",
                "valid_from": "2026-01-01",
                "valid_to": "2027-12-31",
                "signed_at": "2026-02-01"
            }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本追加必须 409: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = post_json(
            &router,
            &format!("/admin/contracts/{contract_id}/terminate"),
            &token,
            json!({ "version": 2 }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "TERMINATED");
        assert_eq!(body["data"]["version"], 3);

        let (status, body) = post_json(
            &router,
            &format!("/admin/contracts/{contract_id}/terminate"),
            &token,
            json!({ "version": 2 }),
        )
        .await;
        assert_eq!(status, 409, "终止使用陈旧版本必须 409: {body}");

        let (status, body) = api
            .get("/admin/contracts?contract_no=HT-2026-0099", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["status"], "TERMINATED");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_contract_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/contracts", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_contract_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        // 种子账号只有 role/admin/audit_log.list 权限，本域权限未授予 → 403。
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/contracts", Some(&token)).await;
        assert_eq!(status, 403, "无 contract.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_unknown_sort_field_rejected() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_contract_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (status, body) = post_json(
            &router,
            "/admin/contracts",
            &token,
            json!({
                "contract_no": "  ",
                "customer_id": customer_id,
                "settlement_party_id": "party-c-g4-1",
                "contract_pdf_file_id": "file-c-g4-1",
                "customer_name": "东方企业",
                "settlement_party_name": "集团结算中心",
                "payment_term_code": "NET30",
                "payment_term_name": "月结 30 天",
                "invoice_type": "增值税专用发票",
                "tax_point": "6",
                "valid_from": "2026-01-01",
                "valid_to": "2026-12-31",
                "signed_at": "2025-12-20"
            }),
        )
        .await;
        assert_eq!(status, 400, "空白合同编号必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null, "失败时 data 恒为 null");

        let (status, _) = api
            .get("/admin/contracts?sort_by=name&sort_dir=asc", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400");

        let (status, _) = api.get("/admin/contracts?page_size=1000", Some(&token)).await;
        assert_eq!(status, 400, "越界分页大小必须 400");
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_contract_no_returns_409_and_pagination_boundary() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_contract_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        for no in ["HT-2026-0101", "HT-2026-0102", "HT-2026-0103"] {
            let (status, body) = post_json(
                &router,
                "/admin/contracts",
                &token,
                contract_payload(&customer_id, no),
            )
            .await;
            assert_ok_envelope(status, &body);
        }

        let (status, body) = post_json(
            &router,
            "/admin/contracts",
            &token,
            contract_payload(&customer_id, "HT-2026-0101"),
        )
        .await;
        assert_eq!(status, 409, "重复 contract_no 唯一索引冲突必须 409: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .get(
                "/admin/contracts?page=2&page_size=2&sort_by=contract_no&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 3);
        assert_eq!(data["page"], 2);
        assert_eq!(data["page_size"], 2);
        assert_eq!(data["items"].as_array().unwrap().len(), 1, "第二页只剩一条");
        assert_eq!(
            data["items"][0]["contract_no"], "HT-2026-0103",
            "按编号升序最后一页为 0103"
        );
    })
}
